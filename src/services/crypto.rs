//! Column encryption for `zeshicast.db` (Q6.3).
//!
//! Sensitive values are sealed with ChaCha20-Poly1305 under a fresh random
//! nonce: `zc1 ‖ nonce(12) ‖ ciphertext+tag`. Equal plaintexts therefore
//! give different blobs, so lookups (clipboard de-duplication, delete by
//! text) go through a keyed fingerprint, HMAC-SHA256 under a second key
//! derived from the same master key.

use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};
use hmac::{Hmac, Mac};
use sha2::Sha256;

const MAGIC: &[u8] = b"zc1";
const NONCE_LEN: usize = 12;
pub(crate) const KEY_LEN: usize = 32;

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CryptoError {
    /// Not a sealed value, or sealed with another key, or tampered with.
    Undecryptable,
    /// The sealed text is not UTF-8 (a corrupt row).
    NotText,
}

impl std::fmt::Display for CryptoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Undecryptable => f.write_str("a stored value cannot be decrypted with this key"),
            Self::NotText => f.write_str("a decrypted value is not text"),
        }
    }
}

pub(crate) struct Cipher {
    aead: ChaCha20Poly1305,
    mac_key: [u8; KEY_LEN],
}

fn derive(master: &[u8; KEY_LEN], label: &[u8]) -> [u8; KEY_LEN] {
    let mut mac = <HmacSha256 as Mac>::new_from_slice(master).expect("HMAC takes any key length");
    mac.update(label);
    mac.finalize().into_bytes().into()
}

pub(crate) fn random_bytes<const N: usize>() -> std::io::Result<[u8; N]> {
    let mut bytes = [0u8; N];
    let mut filled = 0;
    while filled < N {
        filled +=
            rustix::rand::getrandom(&mut bytes[filled..], rustix::rand::GetRandomFlags::empty())?;
    }
    Ok(bytes)
}

impl Cipher {
    pub(crate) fn new(master: &[u8; KEY_LEN]) -> Self {
        let enc = derive(master, b"zeshicast db encryption v1");
        Self {
            aead: ChaCha20Poly1305::new(Key::from_slice(&enc)),
            mac_key: derive(master, b"zeshicast db fingerprint v1"),
        }
    }

    pub(crate) fn seal(&self, plaintext: &[u8]) -> std::io::Result<Vec<u8>> {
        let nonce = random_bytes::<NONCE_LEN>()?;
        let sealed = self
            .aead
            .encrypt(Nonce::from_slice(&nonce), plaintext)
            .map_err(|_| std::io::Error::other("encryption failed"))?;
        let mut out = Vec::with_capacity(MAGIC.len() + NONCE_LEN + sealed.len());
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&nonce);
        out.extend_from_slice(&sealed);
        Ok(out)
    }

    pub(crate) fn open(&self, blob: &[u8]) -> Result<Vec<u8>, CryptoError> {
        let rest = blob.strip_prefix(MAGIC).ok_or(CryptoError::Undecryptable)?;
        if rest.len() < NONCE_LEN {
            return Err(CryptoError::Undecryptable);
        }
        let (nonce, sealed) = rest.split_at(NONCE_LEN);
        self.aead
            .decrypt(Nonce::from_slice(nonce), sealed)
            .map_err(|_| CryptoError::Undecryptable)
    }

    pub(crate) fn open_text(&self, blob: &[u8]) -> Result<String, CryptoError> {
        String::from_utf8(self.open(blob)?).map_err(|_| CryptoError::NotText)
    }

    /// Keyed fingerprint for equality lookups; reveals nothing without the key.
    pub(crate) fn fingerprint(&self, data: &[u8]) -> Vec<u8> {
        let mut mac =
            <HmacSha256 as Mac>::new_from_slice(&self.mac_key).expect("HMAC takes any key length");
        mac.update(data);
        mac.finalize().into_bytes().to_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sealed_text_round_trips_and_hides_the_plaintext() {
        let cipher = Cipher::new(&[7; KEY_LEN]);
        let a = cipher.seal(b"hunter2").unwrap();
        let b = cipher.seal(b"hunter2").unwrap();
        assert_ne!(a, b, "a fresh nonce per value");
        assert!(!a.windows(7).any(|w| w == b"hunter2"));
        assert_eq!(cipher.open_text(&a).unwrap(), "hunter2");
        assert_eq!(
            cipher.fingerprint(b"hunter2"),
            cipher.fingerprint(b"hunter2")
        );
    }

    #[test]
    fn another_key_or_a_flipped_bit_does_not_decrypt() {
        let sealed = Cipher::new(&[7; KEY_LEN]).seal(b"secret").unwrap();
        assert_eq!(
            Cipher::new(&[8; KEY_LEN]).open(&sealed),
            Err(CryptoError::Undecryptable)
        );
        let mut tampered = sealed.clone();
        *tampered.last_mut().unwrap() ^= 1;
        assert_eq!(
            Cipher::new(&[7; KEY_LEN]).open(&tampered),
            Err(CryptoError::Undecryptable)
        );
        assert_eq!(
            Cipher::new(&[7; KEY_LEN]).open(b"plain text"),
            Err(CryptoError::Undecryptable)
        );
        assert_ne!(
            Cipher::new(&[7; KEY_LEN]).fingerprint(b"x"),
            Cipher::new(&[8; KEY_LEN]).fingerprint(b"x")
        );
    }
}
