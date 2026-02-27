
use crate::error::AppError;

pub fn compute_sha256(data: &[u8]) -> [u8; 32] {
    use sha2::{Sha256, Digest};
    Sha256::digest(data).into()
}

pub fn verify_client_hash(file_bytes: &[u8], client_hash_hex: &str)
    -> Result<[u8; 32], AppError>
{
    let server_hash = compute_sha256(file_bytes);
    let client_hash = hex::decode(client_hash_hex)
        .map_err(|_| AppError::InvalidHash)?;
    
    if server_hash.as_ref() != client_hash.as_slice() {
        return Err(AppError::HashMismatch);
    }
    Ok(server_hash)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_sha256_empty_string() {
        let data = b"";
        let hash = compute_sha256(data);
        // Known SHA-256 hash of empty string
        let expected = [
            0xe3, 0xb0, 0xc4, 0x42, 0x98, 0xfc, 0x1c, 0x14,
            0x9a, 0xfb, 0xf4, 0xc8, 0x99, 0x6f, 0xb9, 0x24,
            0x27, 0xae, 0x41, 0xe4, 0x64, 0x9b, 0x93, 0x4c,
            0xa4, 0x95, 0x99, 0x1b, 0x78, 0x52, 0xb8, 0x55,
        ];
        assert_eq!(hash, expected);
    }

    #[test]
    fn test_verify_client_hash_correct() {
        let file_bytes = b"test data";
        // Precomputed SHA-256 of "test data"
        let client_hash_hex = "916f0027a575074ce72a331777c3478d6513f786a591bd892da1a577bf2335f9";
        
        let result = verify_client_hash(file_bytes, client_hash_hex);
        assert!(result.is_ok());
        let hash = result.unwrap();
        assert!(!hash.is_empty());
    }

    #[test]
    fn test_verify_client_hash_mismatch() {
        let file_bytes = b"test data";
        let wrong_hash_hex = "0000000000000000000000000000000000000000000000000000000000000000";
        
        let result = verify_client_hash(file_bytes, wrong_hash_hex);
        assert!(matches!(result, Err(AppError::HashMismatch)));
    }

    #[test]
    fn test_verify_client_hash_invalid_hex() {
        let file_bytes = b"test data";
        let invalid_hex = "not_a_valid_hex_string";
        
        let result = verify_client_hash(file_bytes, invalid_hex);
        assert!(matches!(result, Err(AppError::InvalidHash)));
    }
}
