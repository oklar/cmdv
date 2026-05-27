use aes_gcm::aead::{Aead, AeadCore, KeyInit, OsRng};
use aes_gcm::{Aes128Gcm, Key};
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use base64::Engine;
use rand::RngCore;
use zeroize::{Zeroize, ZeroizeOnDrop};

/// Ciphertext + URL fragment key. Zeroized on drop; caller may zeroize fields early after use.
#[derive(Zeroize, ZeroizeOnDrop)]
pub struct PasteEncrypted {
    pub data_b64: String,
    pub key_b64: String,
}

/// AES-128-GCM compatible with `cmdv-web-frontend` `encryptService.ts` (IV || ciphertext, JWK `k`).
pub fn encrypt_paste(plaintext: &str) -> Result<PasteEncrypted, String> {
    let mut key_bytes = [0u8; 16];
    OsRng.fill_bytes(&mut key_bytes);
    let key = Key::<Aes128Gcm>::from_slice(&key_bytes);
    let cipher = Aes128Gcm::new(key);

    let nonce = Aes128Gcm::generate_nonce(&mut OsRng);
    let ciphertext = cipher
        .encrypt(&nonce, plaintext.as_bytes())
        .map_err(|e| e.to_string())?;

    let mut blob = Vec::with_capacity(nonce.len() + ciphertext.len());
    blob.extend_from_slice(&nonce);
    blob.extend_from_slice(&ciphertext);

    let data_b64 = STANDARD.encode(&blob);
    blob.zeroize();

    let key_b64 = URL_SAFE_NO_PAD.encode(key_bytes);
    key_bytes.zeroize();

    Ok(PasteEncrypted { data_b64, key_b64 })
}

/// Decrypt paste ciphertext compatible with web `encryptService.ts` (IV || ciphertext, raw key).
pub fn decrypt_paste(data_b64: &str, key_b64: &str) -> Result<String, String> {
    use aes_gcm::aead::{Aead, KeyInit};
    use aes_gcm::{Aes128Gcm, Nonce};

    let blob = STANDARD.decode(data_b64).map_err(|e| e.to_string())?;
    if blob.len() < 12 {
        return Err("Invalid ciphertext".into());
    }
    let (nonce_bytes, ct) = blob.split_at(12);
    let mut key_bytes = URL_SAFE_NO_PAD.decode(key_b64).map_err(|e| e.to_string())?;
    if key_bytes.len() != 16 {
        key_bytes.zeroize();
        return Err("Invalid encryption key".into());
    }

    let cipher = Aes128Gcm::new_from_slice(&key_bytes).map_err(|e| e.to_string())?;
    key_bytes.zeroize();
    let nonce = Nonce::from_slice(nonce_bytes);
    let plaintext = cipher
        .decrypt(nonce, ct)
        .map_err(|_| "Decryption failed — link may be invalid or already read.".to_string())?;

    String::from_utf8(plaintext).map_err(|_| "Decrypted data is not valid UTF-8".into())
}

#[cfg(test)]
mod tests {
    use base64::Engine;

    use super::*;

    #[test]
    fn rust_encrypt_decrypt_roundtrip() {
        let plain = "hello from cmdv";
        let enc = encrypt_paste(plain).unwrap();
        let dec = decrypt_paste(&enc.data_b64, &enc.key_b64).unwrap();
        assert_eq!(dec, plain);
    }

    #[test]
    fn encrypt_produces_url_safe_key_and_standard_data_blob() {
        let encrypted = encrypt_paste("hello").unwrap();
        assert!(!encrypted.data_b64.contains('-'));
        assert!(!encrypted.key_b64.contains('+'));
        assert!(!encrypted.key_b64.contains('/'));
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(&encrypted.data_b64)
            .unwrap();
        assert!(decoded.len() >= 12);
    }

    #[test]
    fn key_b64_is_16_byte_aes128_raw_key() {
        let encrypted = encrypt_paste("hello").unwrap();
        let key_bytes = URL_SAFE_NO_PAD.decode(&encrypted.key_b64).unwrap();
        assert_eq!(key_bytes.len(), 16);
    }
}
