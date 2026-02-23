const API_KEY_PREFIXES: &[&str] = &[
    "sk_live_",
    "sk_test_",
    "pk_live_",
    "pk_test_",
    "AKIA",
    "ghp_",
    "gho_",
    "github_pat_",
    "glpat-",
    "Bearer ",
    "xoxb-",
    "xoxp-",
    "sk-",
    "rk_live_",
    "whsec_",
];

const PRIVATE_KEY_MARKERS: &[&str] = &[
    "-----BEGIN RSA PRIVATE KEY-----",
    "-----BEGIN PRIVATE KEY-----",
    "-----BEGIN EC PRIVATE KEY-----",
    "-----BEGIN OPENSSH PRIVATE KEY-----",
    "-----BEGIN PGP PRIVATE KEY BLOCK-----",
];

const CONNECTION_STRING_PREFIXES: &[&str] = &[
    "postgres://",
    "postgresql://",
    "mysql://",
    "mongodb://",
    "mongodb+srv://",
    "redis://",
    "rediss://",
    "amqp://",
    "amqps://",
];

pub fn is_sensitive(content: &str) -> bool {
    has_api_key_prefix(content)
        || has_private_key(content)
        || has_connection_string(content)
        || looks_like_credit_card(content)
        || is_high_entropy_short_string(content)
}

fn has_api_key_prefix(content: &str) -> bool {
    API_KEY_PREFIXES.iter().any(|prefix| content.contains(prefix))
}

fn has_private_key(content: &str) -> bool {
    PRIVATE_KEY_MARKERS.iter().any(|marker| content.contains(marker))
}

fn has_connection_string(content: &str) -> bool {
    CONNECTION_STRING_PREFIXES
        .iter()
        .any(|prefix| content.contains(prefix))
}

fn looks_like_credit_card(content: &str) -> bool {
    let digits: String = content.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.len() < 13 || digits.len() > 19 {
        return false;
    }
    luhn_check(&digits)
}

fn luhn_check(digits: &str) -> bool {
    let sum: u32 = digits
        .chars()
        .rev()
        .enumerate()
        .map(|(i, c)| {
            let mut d = c.to_digit(10).unwrap_or(0);
            if i % 2 == 1 {
                d *= 2;
                if d > 9 {
                    d -= 9;
                }
            }
            d
        })
        .sum();
    sum % 10 == 0
}

fn is_high_entropy_short_string(content: &str) -> bool {
    let trimmed = content.trim();
    if trimmed.len() < 12 || trimmed.len() > 128 {
        return false;
    }
    if trimmed.contains(' ') {
        return false;
    }
    let entropy = shannon_entropy(trimmed);
    entropy > 4.0
}

fn shannon_entropy(s: &str) -> f64 {
    let len = s.len() as f64;
    if len == 0.0 {
        return 0.0;
    }
    let mut freq = [0u32; 256];
    for byte in s.bytes() {
        freq[byte as usize] += 1;
    }
    freq.iter()
        .filter(|&&count| count > 0)
        .map(|&count| {
            let p = count as f64 / len;
            -p * p.log2()
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_api_keys() {
        assert!(is_sensitive("sk_live_abc123def456"));
        assert!(is_sensitive("ghp_abcdefghijklmnop"));
        assert!(is_sensitive("AKIAIOSFODNN7EXAMPLE"));
    }

    #[test]
    fn detects_private_keys() {
        assert!(is_sensitive("-----BEGIN RSA PRIVATE KEY-----\nMIIE..."));
    }

    #[test]
    fn detects_connection_strings() {
        assert!(is_sensitive("postgres://user:pass@localhost/db"));
        assert!(is_sensitive("mongodb+srv://admin:pwd@cluster.mongodb.net"));
    }

    #[test]
    fn detects_credit_cards() {
        assert!(looks_like_credit_card("4532015112830366"));
        assert!(!looks_like_credit_card("1234567890123456"));
    }

    #[test]
    fn normal_text_not_sensitive() {
        assert!(!is_sensitive("Hello, this is a normal sentence."));
        assert!(!is_sensitive("The quick brown fox jumps over the lazy dog"));
    }

    #[test]
    fn high_entropy_detected() {
        assert!(is_high_entropy_short_string(
            "aB3$xK9!mQ7@pL2#nR5&wE8*"
        ));
    }

    #[test]
    fn low_entropy_not_detected() {
        assert!(!is_high_entropy_short_string("aaaaaaaaaaaaa"));
    }

    #[test]
    fn luhn_valid() {
        assert!(luhn_check("4532015112830366"));
        assert!(luhn_check("79927398713"));
    }

    #[test]
    fn luhn_invalid() {
        assert!(!luhn_check("1234567890123456"));
    }
}
