use crate::{Error, Result};
use bip39::{Language, Mnemonic};
use rand::{rngs::OsRng, RngCore};
use zeroize::Zeroizing;

pub fn generate(words: u8, prefix: &str, raw_bits: Option<&str>) -> Result<Zeroizing<String>> {
    let bit_count = match words {
        12 => 128,
        24 => 256,
        _ => return Err(Error::Invalid("choose 12 or 24 words")),
    };
    let mut entropy = Zeroizing::new(vec![0u8; bit_count / 8]);
    if let Some(bits) = raw_bits {
        if !prefix.trim().is_empty()
            || bits.len() != bit_count
            || !bits.bytes().all(|b| b == b'0' || b == b'1')
        {
            return Err(Error::Invalid(
                "raw entropy must be exactly 128 or 256 binary digits, with no prefix",
            ));
        }
        for (i, b) in bits.bytes().enumerate() {
            entropy[i / 8] |= (b - b'0') << (7 - (i % 8));
        }
    } else {
        OsRng
            .try_fill_bytes(&mut entropy)
            .map_err(|_| Error::Invalid("OS randomness unavailable"))?;
        let prefix: Vec<&str> = prefix.split_whitespace().collect();
        if prefix.len() > bit_count / 11 {
            return Err(Error::Invalid("too many manual prefix words"));
        }
        for (i, word) in prefix.iter().enumerate() {
            let index = Language::English.find_word(word).ok_or(Error::Invalid(
                "manual prefix contains an unknown BIP39 word",
            ))? as usize;
            for bit in 0..11 {
                let position = i * 11 + bit;
                let mask = 1u8 << (7 - position % 8);
                entropy[position / 8] = (entropy[position / 8] & !mask)
                    | ((((index >> (10 - bit)) & 1) as u8) << (7 - position % 8));
            }
        }
    }
    let mnemonic =
        Mnemonic::from_entropy(&entropy).map_err(|_| Error::Invalid("entropy length"))?;
    Ok(Zeroizing::new(mnemonic.to_string()))
}
pub fn validate(phrase: &str) -> Result<Zeroizing<String>> {
    if phrase.len() > 1024 {
        return Err(Error::Invalid("phrase too long"));
    }
    let words = input_words(phrase)?;
    let mnemonic = Mnemonic::parse_in(Language::English, &*words)
        .map_err(|_| Error::Invalid("BIP39 words or checksum are invalid"))?;
    if !matches!(mnemonic.word_count(), 12 | 24) {
        return Err(Error::Unsupported(
            "only 12 and 24 word phrases are enabled",
        ));
    }
    Ok(Zeroizing::new(mnemonic.to_string()))
}

/// Normalize human-entered English words without guessing typos or reordering them.
fn input_words(phrase: &str) -> Result<Zeroizing<String>> {
    let numbered = phrase.chars().any(|c| c.is_ascii_digit());
    let mut result = Zeroizing::new(String::new());
    let list = Language::English.word_list();
    let mut push_word = |word: &str| -> Result<()> {
        let lower = Zeroizing::new(word.to_ascii_lowercase());
        let full = if list.contains(&lower.as_str()) {
            lower.as_str()
        } else if lower.len() == 4 && lower.bytes().all(|c| c.is_ascii_lowercase()) {
            let mut matches = list.iter().filter(|word| word.starts_with(lower.as_str()));
            let first = matches
                .next()
                .ok_or(Error::Invalid("Unknown recovery word; check its spelling"))?;
            if matches.next().is_some() {
                return Err(Error::Invalid("Recovery word prefix is ambiguous"));
            }
            first
        } else {
            return Err(Error::Invalid(
                "Use complete recovery words or their first four letters",
            ));
        };
        if !result.is_empty() {
            result.push(' ');
        }
        result.push_str(full);
        Ok(())
    };
    if numbered {
        for (index, line) in phrase
            .lines()
            .filter(|line| !line.trim().is_empty())
            .enumerate()
        {
            let (number, word) = line.trim().split_once('.').ok_or(Error::Invalid(
                "Numbered phrases need one word per line, like 1. word",
            ))?;
            if number != (index + 1).to_string() || word.split_whitespace().count() != 1 {
                return Err(Error::Invalid(
                    "Use one numbered word per line in order, starting at 1. Do not put numbered words on the same line",
                ));
            }
            push_word(word.trim())?;
        }
    } else {
        for word in phrase.split_whitespace() {
            push_word(word)?;
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn official_zero_entropy_vectors() {
        assert_eq!(&*generate(12, "", Some(&"0".repeat(128))).unwrap(), "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about");
        assert_eq!(
            generate(24, "", Some(&"0".repeat(256)))
                .unwrap()
                .split_whitespace()
                .last(),
            Some("art")
        );
    }
    #[test]
    fn every_prefix_count_preserves_leading_zero_bits() {
        for words in [12, 24] {
            for count in 0..words as usize {
                let prefix = (0..count)
                    .map(|i| if i % 2 == 0 { "abandon" } else { "zoo" })
                    .collect::<Vec<_>>()
                    .join(" ");
                let phrase = generate(words, &prefix, None).unwrap();
                assert!(phrase.starts_with(&prefix));
                validate(&phrase).unwrap();
            }
        }
    }
    #[test]
    fn human_phrase_formats_are_canonical_and_unambiguous() {
        let phrase = generate(12, "", Some(&"0".repeat(128))).unwrap();
        let numbered = phrase
            .split_whitespace()
            .enumerate()
            .map(|(i, w)| format!("{}. {}", i + 1, &w[..4]))
            .collect::<Vec<_>>()
            .join("\r\n");
        assert_eq!(*validate(&numbered).unwrap(), *phrase);
        assert_eq!(
            *validate(&phrase.replace(' ', "\n").to_uppercase()).unwrap(),
            *phrase
        );
        assert_eq!(
            *validate(
                &phrase
                    .split_whitespace()
                    .map(|w| &w[..4])
                    .collect::<Vec<_>>()
                    .join(" ")
            )
            .unwrap(),
            *phrase
        );
        assert!(validate(&numbered.replace("\r\n", " ")).is_err());
        assert!(validate(&numbered.replacen("2.", "3.", 1)).is_err());
        assert!(validate(&numbered.replacen("2. aban", "2aban", 1)).is_err());
        assert!(validate(&phrase.replacen("abandon", "aba", 1)).is_err());
        assert!(validate(&phrase.replacen("abandon", "abandox", 1)).is_err());
        assert!(validate(&phrase.replace("about", "abandon")).is_err());
        for word in Language::English.word_list() {
            assert_eq!(&*input_words(&word[..word.len().min(4)]).unwrap(), *word);
        }
    }
    #[test]
    fn rejects_invalid_entropy() {
        assert!(generate(24, "notaword", None).is_err());
        assert!(generate(24, "", Some(&"0".repeat(255))).is_err());
        assert!(validate(&"abandon ".repeat(12)).is_err());
    }
}
