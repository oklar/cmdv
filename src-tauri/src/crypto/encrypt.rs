use aes_gcm::{
    aead::{Aead, KeyInit, OsRng},
    AeadCore, Aes256Gcm, Nonce,
};

pub fn encrypt(key: &[u8; 32], plaintext: &[u8]) -> Result<(Vec<u8>, Vec<u8>), String> {
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|e| e.to_string())?;
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
    let ciphertext = cipher
        .encrypt(&nonce, plaintext)
        .map_err(|e| e.to_string())?;
    Ok((nonce.to_vec(), ciphertext))
}

pub fn decrypt(key: &[u8; 32], nonce_bytes: &[u8], ciphertext: &[u8]) -> Result<Vec<u8>, String> {
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|e| e.to_string())?;
    let nonce = Nonce::from_slice(nonce_bytes);
    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|_| "Decryption failed: invalid key or corrupted data".to_string())?;
    Ok(plaintext)
}

pub fn encrypt_blob(key: &[u8; 32], data: &[u8]) -> Result<Vec<u8>, String> {
    let (nonce, ciphertext) = encrypt(key, data)?;
    let mut blob = Vec::with_capacity(12 + ciphertext.len());
    blob.extend_from_slice(&nonce);
    blob.extend_from_slice(&ciphertext);
    Ok(blob)
}

pub fn decrypt_blob(key: &[u8; 32], blob: &[u8]) -> Result<Vec<u8>, String> {
    if blob.len() < 12 {
        return Err("Blob too short to contain nonce".into());
    }
    let (nonce, ciphertext) = blob.split_at(12);
    decrypt(key, nonce, ciphertext)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::RngCore;

    fn random_key() -> [u8; 32] {
        let mut key = [0u8; 32];
        OsRng.fill_bytes(&mut key);
        key
    }

    #[test]
    fn encrypt_decrypt_roundtrip() {
        let key = random_key();
        let plaintext = b"Hello, clipboard!";
        let (nonce, ciphertext) = encrypt(&key, plaintext).unwrap();
        let decrypted = decrypt(&key, &nonce, &ciphertext).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn decrypt_with_wrong_key_fails() {
        let key1 = random_key();
        let key2 = random_key();
        let (nonce, ciphertext) = encrypt(&key1, b"secret").unwrap();
        assert!(decrypt(&key2, &nonce, &ciphertext).is_err());
    }

    #[test]
    fn nonces_are_unique() {
        let key = random_key();
        let (nonce1, _) = encrypt(&key, b"data1").unwrap();
        let (nonce2, _) = encrypt(&key, b"data2").unwrap();
        assert_ne!(nonce1, nonce2);
    }

    #[test]
    fn blob_encrypt_decrypt_roundtrip() {
        let key = random_key();
        let data = b"blob content test";
        let blob = encrypt_blob(&key, data).unwrap();
        let decrypted = decrypt_blob(&key, &blob).unwrap();
        assert_eq!(decrypted, data);
    }

    #[test]
    fn blob_too_short_fails() {
        let key = random_key();
        assert!(decrypt_blob(&key, &[0u8; 5]).is_err());
    }

    #[test]
    fn tampered_ciphertext_fails() {
        let key = random_key();
        let (nonce, mut ciphertext) = encrypt(&key, b"important data").unwrap();
        if let Some(byte) = ciphertext.last_mut() {
            *byte ^= 0xFF;
        }
        assert!(decrypt(&key, &nonce, &ciphertext).is_err());
    }
}
