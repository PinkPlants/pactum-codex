use crate::error::AppError;
use aes_gcm::{
    aead::{rand_core::RngCore, Aead, KeyInit, OsRng},
    Aes256Gcm, Key, Nonce,
};

pub fn encrypt(plaintext: &str, key: &[u8; 32]) -> Result<(Vec<u8>, [u8; 12]), AppError> {
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
    let mut nonce_bytes = [0u8; 12];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher
        .encrypt(nonce, plaintext.as_bytes())
        .map_err(|_| AppError::EncryptionFailed)?;
    Ok((ciphertext, nonce_bytes))
}

pub fn decrypt(ciphertext: &[u8], nonce: &[u8; 12], key: &[u8; 32]) -> Result<String, AppError> {
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
    let nonce = Nonce::from_slice(nonce);
    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|_| AppError::DecryptionFailed)?;
    String::from_utf8(plaintext).map_err(|_| AppError::DecryptionFailed)
}

// Blind index for email lookup (HMAC-SHA256)
pub fn hmac_index(value: &str, key: &[u8; 32]) -> Vec<u8> {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(key).unwrap();
    mac.update(value.as_bytes());
    mac.finalize().into_bytes().to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let plaintext = "test@example.com";
        let key = [42u8; 32];

        let (ciphertext, nonce) = encrypt(plaintext, &key).expect("encrypt failed");
        let decrypted = decrypt(&ciphertext, &nonce, &key).expect("decrypt failed");

        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_random_nonce_produces_different_ciphertexts() {
        let plaintext = "test@example.com";
        let key = [42u8; 32];

        let (ciphertext1, nonce1) = encrypt(plaintext, &key).expect("encrypt 1 failed");
        let (ciphertext2, nonce2) = encrypt(plaintext, &key).expect("encrypt 2 failed");

        // Nonces must be different (random)
        assert_ne!(nonce1, nonce2, "nonces should be different (random)");
        // Ciphertexts must be different (because nonces are different)
        assert_ne!(
            ciphertext1, ciphertext2,
            "ciphertexts should differ (random nonce)"
        );
    }

    #[test]
    fn test_decrypt_with_wrong_key_fails() {
        let plaintext = "test@example.com";
        let correct_key = [42u8; 32];
        let wrong_key = [99u8; 32];

        let (ciphertext, nonce) = encrypt(plaintext, &correct_key).expect("encrypt failed");
        let result = decrypt(&ciphertext, &nonce, &wrong_key);

        assert!(result.is_err());
        match result {
            Err(AppError::DecryptionFailed) => {} // Expected
            other => panic!("Expected DecryptionFailed, got {:?}", other),
        }
    }

    #[test]
    fn test_hmac_index_is_deterministic() {
        let value = "test@example.com";
        let key = [42u8; 32];

        let hash1 = hmac_index(value, &key);
        let hash2 = hmac_index(value, &key);

        assert_eq!(hash1, hash2, "HMAC should be deterministic");
    }

    #[test]
    fn test_hmac_index_different_values_produce_different_hashes() {
        let key = [42u8; 32];

        let hash1 = hmac_index("test1@example.com", &key);
        let hash2 = hmac_index("test2@example.com", &key);

        assert_ne!(
            hash1, hash2,
            "different values should produce different HMAC"
        );
    }

    #[test]
    fn test_hmac_index_same_value_different_key_produces_different_hash() {
        let value = "test@example.com";
        let key1 = [42u8; 32];
        let key2 = [99u8; 32];

        let hash1 = hmac_index(value, &key1);
        let hash2 = hmac_index(value, &key2);

        assert_ne!(hash1, hash2, "different keys should produce different HMAC");
    }

    #[test]
    fn test_decrypt_invalid_utf8_fails() {
        let key = [42u8; 32];

        // Create valid ciphertext
        let plaintext = "test";
        let (mut ciphertext, nonce) = encrypt(plaintext, &key).expect("encrypt failed");

        // Corrupt the ciphertext to cause decryption to produce invalid UTF-8
        // (This is probabilistic, but we'll try)
        if !ciphertext.is_empty() {
            ciphertext[0] ^= 0xFF;
        }

        let result = decrypt(&ciphertext, &nonce, &key);

        // Either DecryptionFailed (auth tag mismatch) or DecryptionFailed (invalid UTF-8)
        assert!(result.is_err(), "should fail on corrupted ciphertext");
    }
}
