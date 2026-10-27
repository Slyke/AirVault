use airvault_core::entropy;
use sha2::{Digest, Sha256};

// Independent reconstruction: no bip39 parser participates in these assertions.
fn check(phrase: &str, prefix: &str, expected_entropy: Option<&str>) {
    let list: Vec<_> = include_str!("../../../src/assets/bip39-english.txt")
        .lines()
        .collect();
    let words: Vec<_> = phrase.split_whitespace().collect();
    assert!(words.len() == 12 || words.len() == 24);
    let bits: String = words
        .iter()
        .map(|word| format!("{:011b}", list.iter().position(|w| w == word).unwrap()))
        .collect();
    let entropy_len = if words.len() == 24 { 256 } else { 128 };
    let bytes: Vec<u8> = bits[..entropy_len]
        .as_bytes()
        .chunks(8)
        .map(|byte| byte.iter().fold(0, |sum, bit| (sum << 1) | (bit - b'0')))
        .collect();
    let hash = Sha256::digest(&bytes);
    let checksum = format!("{:08b}", hash[0]);
    assert_eq!(&bits[entropy_len..], &checksum[..entropy_len / 32]);
    if let Some(expected) = expected_entropy {
        assert_eq!(&bits[..entropy_len], expected);
    }
    let prefix_bits: String = prefix
        .split_whitespace()
        .map(|word| format!("{:011b}", list.iter().position(|w| *w == word).unwrap()))
        .collect();
    assert!(bits.starts_with(&prefix_bits));
    assert!(phrase.starts_with(prefix.trim()));
}
#[test]
fn official_list_matches_native_generator_and_every_prefix_preserves_entropy() {
    let list: Vec<_> = include_str!("../../../src/assets/bip39-english.txt")
        .lines()
        .collect();
    assert_eq!(list.as_slice(), bip39::Language::English.word_list());
    for words in [12, 24] {
        for count in 0..words {
            let prefix = (0..count)
                .map(|i| list[(i as usize * 137) % 2048])
                .collect::<Vec<_>>()
                .join(" ");
            check(
                &entropy::generate(words, &prefix, None).unwrap(),
                &prefix,
                None,
            );
        }
    }
}
#[test]
fn fully_dice_entropy_and_checksum_are_independently_reconstructed() {
    for words in [12, 24] {
        let len = if words == 24 { 256 } else { 128 };
        for pattern in ["0", "1", "00110101"] {
            let bits = pattern.repeat(len / pattern.len());
            check(
                &entropy::generate(words, "", Some(&bits)).unwrap(),
                "",
                Some(&bits),
            );
        }
    }
}
