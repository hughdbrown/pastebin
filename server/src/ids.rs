//! Short-ID generation for shareable paste links.

use rand::Rng;

/// Base62 alphabet (digits + upper + lower). 62 symbols => ~5.95 bits each.
const ALPHABET: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";

/// Default short-id length. 8 base62 chars ~= 47.6 bits of entropy, enough that
/// `unlisted` pastes are not practically guessable.
pub const DEFAULT_LEN: usize = 8;

/// Generate a random base62 short id of the given length.
pub fn generate(len: usize) -> String {
    let mut rng = rand::thread_rng();
    (0..len)
        .map(|_| {
            let idx = rng.gen_range(0..ALPHABET.len());
            ALPHABET[idx] as char
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_requested_length() {
        assert_eq!(generate(8).len(), 8);
        assert_eq!(generate(12).len(), 12);
    }

    #[test]
    fn uses_only_base62_characters() {
        let id = generate(64);
        assert!(id.bytes().all(|b| ALPHABET.contains(&b)));
    }

    #[test]
    fn is_reasonably_unique() {
        let mut seen = std::collections::HashSet::new();
        for _ in 0..1000 {
            assert!(seen.insert(generate(DEFAULT_LEN)), "unexpected collision");
        }
    }
}
