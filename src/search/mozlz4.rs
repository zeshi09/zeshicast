//! Minimal reader for the `mozLz4` container Firefox writes session stores in.
//!
//! The format is an 8-byte `mozLz40\0` magic, a little-endian uncompressed size
//! and then a raw LZ4 *block* (the block format, not the frame format). A block
//! decoder is about fifty lines, which is cheaper than a dependency for one file
//! and keeps it auditable -- the input is a file we did not write.
//!
//! The decoder is deliberately strict: a declared size is respected, every read
//! is bounds-checked, and a malformed block is an error rather than a partial
//! session store.

const MAGIC: &[u8; 8] = b"mozLz40\0";
const HEADER_BYTES: usize = 12;
/// Firefox session stores are a few megabytes; this bounds what a corrupt header
/// can make us allocate.
const MAX_UNCOMPRESSED: usize = 64 * 1024 * 1024;

/// Decode a `mozLz4` container into the text it holds.
pub(crate) fn decompress(bytes: &[u8]) -> Result<String, String> {
    if bytes.len() < HEADER_BYTES {
        return Err("truncated mozLz4 header".to_string());
    }
    if &bytes[..8] != MAGIC {
        return Err("not a mozLz4 container".to_string());
    }

    let size = u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]) as usize;
    if size > MAX_UNCOMPRESSED {
        return Err(format!("refusing to allocate {size} bytes"));
    }

    let decoded = decompress_block(&bytes[HEADER_BYTES..], size)?;
    String::from_utf8(decoded).map_err(|error| format!("session store is not UTF-8: {error}"))
}

/// Decode one LZ4 block that is expected to produce exactly `size` bytes.
fn decompress_block(input: &[u8], size: usize) -> Result<Vec<u8>, String> {
    let mut out: Vec<u8> = Vec::with_capacity(size.min(1 << 20));
    let mut pos = 0usize;

    while pos < input.len() {
        let token = input[pos];
        pos += 1;

        let mut literals = (token >> 4) as usize;
        if literals == 15 {
            loop {
                let Some(&extra) = input.get(pos) else {
                    return Err("truncated literal length".to_string());
                };
                pos += 1;
                literals += extra as usize;
                if extra != 255 {
                    break;
                }
            }
        }
        if input.len() < pos + literals {
            return Err("truncated literals".to_string());
        }
        out.extend_from_slice(&input[pos..pos + literals]);
        pos += literals;

        // The last sequence of a block holds literals only.
        if pos == input.len() {
            break;
        }

        let offset = match (input.get(pos).copied(), input.get(pos + 1).copied()) {
            (Some(low), Some(high)) => u16::from_le_bytes([low, high]) as usize,
            _ => return Err("truncated match offset".to_string()),
        };
        pos += 2;
        if offset == 0 || offset > out.len() {
            return Err(format!("match offset {offset} is out of range"));
        }

        let mut length = (token & 0x0F) as usize;
        if length == 15 {
            loop {
                let Some(&extra) = input.get(pos) else {
                    return Err("truncated match length".to_string());
                };
                pos += 1;
                length += extra as usize;
                if extra != 255 {
                    break;
                }
            }
        }
        length += 4;
        if out.len() + length > size {
            return Err("block expands past the declared size".to_string());
        }

        // Byte-by-byte on purpose: matches routinely overlap the bytes they are
        // copying (a run of one repeated character is offset 1, length > 1).
        let start = out.len() - offset;
        for index in 0..length {
            let byte = out[start + index];
            out.push(byte);
        }
    }

    if out.len() != size {
        return Err(format!(
            "block produced {} bytes, expected {size}",
            out.len()
        ));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn container(block: &[u8], size: usize) -> Vec<u8> {
        let mut bytes = MAGIC.to_vec();
        bytes.extend_from_slice(&(size as u32).to_le_bytes());
        bytes.extend_from_slice(block);
        bytes
    }

    #[test]
    fn literal_only_block_is_decoded() {
        // token high nibble = 11 literals, no match follows.
        let mut block = vec![11 << 4];
        block.extend_from_slice(b"hello world");
        assert_eq!(decompress(&container(&block, 11)).unwrap(), "hello world");
    }

    #[test]
    fn long_literal_run_uses_the_extra_length_bytes() {
        let text = b"0123456789abcdefghij"; // 20 bytes: 15 + 5
        let mut block = vec![0xF0, 5];
        block.extend_from_slice(text);
        assert_eq!(
            decompress(&container(&block, 20)).unwrap(),
            "0123456789abcdefghij"
        );
    }

    #[test]
    fn a_match_copies_from_earlier_output() {
        // "abc" + match(offset 3, length 6) = "abcabcabc".
        let mut block = vec![(3 << 4) | 2];
        block.extend_from_slice(b"abc");
        block.extend_from_slice(&3u16.to_le_bytes());
        assert_eq!(decompress(&container(&block, 9)).unwrap(), "abcabcabc");
    }

    #[test]
    fn an_overlapping_match_repeats_a_single_byte() {
        // "x" then match(offset 1, length 19): offset 1 means each copied byte is
        // the one just written, which is what a run of one character looks like.
        let mut block = vec![(1 << 4) | 15];
        block.extend_from_slice(b"x");
        block.extend_from_slice(&1u16.to_le_bytes());
        block.push(0); // 15 + 0 + 4 = 19

        let decoded = decompress(&container(&block, 20)).expect("a valid overlapping match");
        assert_eq!(decoded, "x".repeat(20));
    }

    #[test]
    fn a_foreign_or_truncated_container_is_rejected() {
        assert!(decompress(b"short").is_err());
        assert!(decompress(b"not-mozlz4, but long enough").is_err());

        let block = vec![11 << 4];
        let mut container = container(&block, 11);
        container.truncate(container.len() - 1);
        assert!(
            decompress(&container).is_err(),
            "a block that produces too little is an error, not a partial store"
        );
    }

    #[test]
    fn an_out_of_range_match_offset_is_rejected() {
        let mut block = vec![(1 << 4) | 2];
        block.extend_from_slice(b"a");
        block.extend_from_slice(&9u16.to_le_bytes()); // nothing 9 bytes back yet
        assert!(decompress(&container(&block, 7)).is_err());
    }

    #[test]
    fn a_block_larger_than_declared_is_rejected() {
        let mut block = vec![20 << 4];
        block.extend_from_slice(b"0123456789abcdefghij");
        assert!(
            decompress(&container(&block, 5)).is_err(),
            "the declared size bounds the output"
        );
    }

    #[test]
    fn an_absurd_declared_size_is_refused_before_allocating() {
        let mut container = container(&[], u32::MAX as usize);
        container.truncate(12);
        assert!(decompress(&container).is_err());
    }
}
