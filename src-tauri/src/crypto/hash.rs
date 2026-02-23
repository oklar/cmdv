pub fn keyed_hash(key: &[u8; 32], data: &[u8]) -> Vec<u8> {
    blake3::keyed_hash(key, data).as_bytes().to_vec()
}

pub fn content_hash_hex(key: &[u8; 32], data: &[u8]) -> String {
    hex::encode(keyed_hash(key, data))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyed_hash_deterministic() {
        let key = [42u8; 32];
        let data = b"test data";
        let h1 = keyed_hash(&key, data);
        let h2 = keyed_hash(&key, data);
        assert_eq!(h1, h2);
    }

    #[test]
    fn different_keys_produce_different_hashes() {
        let key1 = [1u8; 32];
        let key2 = [2u8; 32];
        let data = b"same data";
        let h1 = keyed_hash(&key1, data);
        let h2 = keyed_hash(&key2, data);
        assert_ne!(h1, h2);
    }

    #[test]
    fn different_data_produce_different_hashes() {
        let key = [1u8; 32];
        let h1 = keyed_hash(&key, b"data one");
        let h2 = keyed_hash(&key, b"data two");
        assert_ne!(h1, h2);
    }

    #[test]
    fn hash_is_32_bytes() {
        let key = [0u8; 32];
        let h = keyed_hash(&key, b"anything");
        assert_eq!(h.len(), 32);
    }

    #[test]
    fn hex_encoding_works() {
        let key = [0u8; 32];
        let hex_str = content_hash_hex(&key, b"data");
        assert_eq!(hex_str.len(), 64);
        assert!(hex_str.chars().all(|c| c.is_ascii_hexdigit()));
    }
}
