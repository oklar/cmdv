use bip39::{Language, Mnemonic};
use rand::RngCore;
use zeroize::Zeroize;

pub fn generate_mnemonic_24() -> Result<(String, Vec<u8>), String> {
    let mut entropy = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut entropy);
    let mnemonic = Mnemonic::from_entropy(&entropy).map_err(|e| e.to_string())?;
    let words = mnemonic.to_string();
    let entropy_vec = entropy.to_vec();
    entropy.zeroize();
    Ok((words, entropy_vec))
}

pub fn validate_mnemonic(phrase: &str) -> Result<Vec<u8>, String> {
    let mnemonic = Mnemonic::parse_in(Language::English, phrase).map_err(|e| e.to_string())?;
    Ok(mnemonic.to_entropy())
}

pub fn words_to_entropy(phrase: &str) -> Result<Vec<u8>, String> {
    validate_mnemonic(phrase)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_produces_24_words() {
        let (words, entropy) = generate_mnemonic_24().unwrap();
        assert_eq!(words.split_whitespace().count(), 24);
        assert_eq!(entropy.len(), 32);
    }

    #[test]
    fn roundtrip_mnemonic() {
        let (words, entropy) = generate_mnemonic_24().unwrap();
        let recovered = validate_mnemonic(&words).unwrap();
        assert_eq!(entropy, recovered);
    }

    #[test]
    fn invalid_mnemonic_fails() {
        assert!(validate_mnemonic("not a valid mnemonic phrase at all").is_err());
    }

    #[test]
    fn generated_mnemonics_are_unique() {
        let (w1, _) = generate_mnemonic_24().unwrap();
        let (w2, _) = generate_mnemonic_24().unwrap();
        assert_ne!(w1, w2);
    }
}
