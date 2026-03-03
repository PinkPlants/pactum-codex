use crate::error::AppError;
use crate::state::AppState;
use crate::state::ProtectedKeypair;
use solana_sdk::signature::Keypair;
use solana_sdk::signer::Signer;

/// Load a keypair from a file path (preferred) or base58 env var (fallback).
/// The file should be mounted as a Docker secret or fetched from a secrets manager
/// at container startup — never baked into the image or committed to git.
pub fn load_keypair(path: &str) -> Result<ProtectedKeypair, AppError> {
    let json =
        std::fs::read_to_string(path).map_err(|e| AppError::KeypairLoadFailed(e.to_string()))?;

    // Solana keypair JSON is a [u8; 64] array
    let bytes: Vec<u8> =
        serde_json::from_str(&json).map_err(|e| AppError::KeypairLoadFailed(e.to_string()))?;

    let keypair_bytes: [u8; 64] = bytes
        .try_into()
        .map_err(|_| AppError::KeypairLoadFailed("Invalid keypair length".to_string()))?;
    let keypair = Keypair::try_from(keypair_bytes.as_slice())
        .map_err(|e| AppError::KeypairLoadFailed(e.to_string()))?;

    Ok(ProtectedKeypair(keypair))
}

/// Called at server startup. Panics if pubkeys do not match config.
/// Catches wrong-file-loaded mistakes before any real transactions are signed.
pub fn validate_keypair_pubkeys(state: &AppState) {
    assert_eq!(
        state.vault_keypair.0.pubkey().to_string(),
        state.config.platform_vault_pubkey,
        "FATAL: vault keypair pubkey does not match PLATFORM_VAULT_PUBKEY — wrong key loaded"
    );
    assert_eq!(
        state.treasury_keypair.0.pubkey().to_string(),
        state.config.platform_treasury_pubkey,
        "FATAL: treasury keypair pubkey does not match PLATFORM_TREASURY_PUBKEY — wrong key loaded"
    );
    tracing::info!("Platform keypairs validated ✓");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;
    use std::path::Path;

    fn create_temp_keypair_file(content: &str) -> String {
        let temp_path = format!("/tmp/test_keypair_{}.json", uuid::Uuid::new_v4());
        let mut file = fs::File::create(&temp_path).expect("Failed to create temp file");
        file.write_all(content.as_bytes())
            .expect("Failed to write to temp file");
        drop(file);
        temp_path
    }

    fn cleanup_temp_file(path: &str) {
        let _ = fs::remove_file(path);
    }

    #[test]
    fn test_load_valid_keypair() {
        // Create a fresh keypair for testing
        let test_keypair = Keypair::new();
        let keypair_bytes = test_keypair.to_bytes();

        // Write as JSON array format (Solana's standard)
        let json_content = format!(
            "[{}]",
            keypair_bytes
                .iter()
                .map(|b| b.to_string())
                .collect::<Vec<_>>()
                .join(",")
        );

        // Create temp file with keypair JSON
        let path = create_temp_keypair_file(&json_content);

        // Load the keypair
        let result = load_keypair(&path);
        assert!(result.is_ok(), "Failed to load valid keypair");

        let loaded = result.unwrap();
        assert_eq!(
            loaded.0.pubkey().to_string(),
            test_keypair.pubkey().to_string(),
            "Loaded pubkey should match original"
        );

        cleanup_temp_file(&path);
    }

    #[test]
    fn test_load_invalid_json() {
        // Create temp file with invalid JSON
        let path = create_temp_keypair_file("invalid json content");

        let result = load_keypair(&path);
        assert!(
            matches!(result, Err(AppError::KeypairLoadFailed(_))),
            "Invalid JSON should return KeypairLoadFailed error"
        );

        cleanup_temp_file(&path);
    }

    #[test]
    fn test_load_file_not_found() {
        let result = load_keypair("/nonexistent/path/to/keypair.json");
        assert!(
            matches!(result, Err(AppError::KeypairLoadFailed(_))),
            "File not found should return KeypairLoadFailed error"
        );
    }

    #[test]
    fn test_load_invalid_keypair_bytes() {
        // Create temp file with valid JSON but invalid keypair bytes (wrong array length)
        // Array with only 32 bytes instead of 64
        let short_array = format!("[{}]", (0..32).map(|_| "0").collect::<Vec<_>>().join(","));
        let path = create_temp_keypair_file(&short_array);

        let result = load_keypair(&path);
        assert!(
            matches!(result, Err(AppError::KeypairLoadFailed(_))),
            "Invalid keypair bytes should return KeypairLoadFailed error"
        );

        cleanup_temp_file(&path);
    }
}
