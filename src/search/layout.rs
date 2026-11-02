//! Wrong-keyboard-layout recovery (Q1.1).
//!
//! Typing `ашкуащч` with the Russian layout active means `firefox`. When a
//! Cyrillic query finds nothing, the search retries it as the same keys on
//! the US QWERTY layout.

/// Russian ЙЦУКЕН → US QWERTY, by physical key.
const JCUKEN_TO_QWERTY: [(char, char); 33] = [
    ('й', 'q'),
    ('ц', 'w'),
    ('у', 'e'),
    ('к', 'r'),
    ('е', 't'),
    ('н', 'y'),
    ('г', 'u'),
    ('ш', 'i'),
    ('щ', 'o'),
    ('з', 'p'),
    ('х', '['),
    ('ъ', ']'),
    ('ф', 'a'),
    ('ы', 's'),
    ('в', 'd'),
    ('а', 'f'),
    ('п', 'g'),
    ('р', 'h'),
    ('о', 'j'),
    ('л', 'k'),
    ('д', 'l'),
    ('ж', ';'),
    ('э', '\''),
    ('я', 'z'),
    ('ч', 'x'),
    ('с', 'c'),
    ('м', 'v'),
    ('и', 'b'),
    ('т', 'n'),
    ('ь', 'm'),
    ('б', ','),
    ('ю', '.'),
    ('ё', '`'),
];

/// The query as typed on QWERTY, or `None` when it has no Cyrillic letter
/// (nothing to recover). Case is folded; non-Cyrillic characters (digits,
/// spaces, punctuation) are kept as they are.
pub(crate) fn qwerty_from_jcuken(query: &str) -> Option<String> {
    let mut changed = false;
    let mapped = query
        .chars()
        .flat_map(char::to_lowercase)
        .map(
            |c| match JCUKEN_TO_QWERTY.iter().find(|(cyrillic, _)| *cyrillic == c) {
                Some((_, latin)) => {
                    changed = true;
                    *latin
                }
                None => c,
            },
        )
        .collect();
    changed.then_some(mapped)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cyrillic_keys_map_to_the_same_qwerty_keys() {
        assert_eq!(qwerty_from_jcuken("ашкуащч").as_deref(), Some("firefox"));
        assert_eq!(qwerty_from_jcuken("ЕУКЬШТФД").as_deref(), Some("terminal"));
        assert_eq!(qwerty_from_jcuken("аш 2").as_deref(), Some("fi 2"));
    }

    #[test]
    fn latin_queries_are_left_alone() {
        assert_eq!(qwerty_from_jcuken("firefox"), None);
        assert_eq!(qwerty_from_jcuken("123 + 4"), None);
        assert_eq!(qwerty_from_jcuken(""), None);
    }
}
