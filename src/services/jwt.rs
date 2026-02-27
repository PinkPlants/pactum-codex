use crate::config::Config;
use crate::error::AppError;
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

/// JWT Claims struct matching spec §7.4
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    pub sub: Uuid,                // user_id
    pub pubkey: Option<String>,   // wallet address if available
    pub exp: usize,               // expiration timestamp in SECONDS (not milliseconds)
    pub iat: usize,               // issued-at timestamp in SECONDS
    pub jti: Uuid,                // JWT ID for tracking/revocation
}

/// Compute SHA-256 hash of a string and return hex-encoded result
pub fn sha256_hex(input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    let hash_bytes = hasher.finalize();
    hex::encode(hash_bytes)
}

/// Get current Unix timestamp in seconds
fn current_timestamp() -> usize {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
        .as_secs() as usize
}

/// Issue an access token (short-lived, 15 minutes)
pub fn issue_access_token(
    user_id: Uuid,
    pubkey: Option<String>,
    config: &Config,
) -> Result<String, AppError> {
    let now = current_timestamp();
    let exp = now + (config.jwt_access_expiry_seconds as usize);

    let claims = Claims {
        sub: user_id,
        pubkey,
        exp,
        iat: now,
        jti: Uuid::new_v4(),
    };

    let encoding_key = EncodingKey::from_secret(config.jwt_secret.as_bytes());
    encode(&Header::default(), &claims, &encoding_key)
        .map_err(|_| AppError::InternalError)
}

/// Decode and validate an access token
pub fn decode_access_token(token: &str, config: &Config) -> Result<Claims, AppError> {
    let decoding_key = DecodingKey::from_secret(config.jwt_secret.as_bytes());
    let validation = Validation::default();

    decode::<Claims>(token, &decoding_key, &validation)
        .map(|data| data.claims)
        .map_err(|_| AppError::Unauthorized)
}

/// Issue and store a refresh token (long-lived, 7 days)
/// Returns the raw token (plaintext); hash is stored in database
pub async fn issue_and_store_refresh_token(
    db: &PgPool,
    user_id: Uuid,
) -> Result<String, AppError> {
    // Generate random refresh token (32 bytes = 256 bits)
    let token_bytes: [u8; 32] = rand::random();
    let raw_token = hex::encode(token_bytes);

    // Hash the token before storage (SHA-256)
    let token_hash = sha256_hex(&raw_token);

    // Get expiry timestamp (7 days from now)
    let now = current_timestamp() as i64;
    let expires_at = now + 604800i64; // 7 days in seconds

    // Store hash in database (NOT the raw token)
    sqlx::query(
        "INSERT INTO refresh_tokens (user_id, token_hash, expires_at, created_at) 
         VALUES ($1, $2, $3, $4)",
    )
    .bind(user_id)
    .bind(&token_hash)
    .bind(expires_at)
    .bind(now)
    .execute(db)
    .await
    .map_err(|_| AppError::InternalError)?;

    // Return the raw token (client will send this back with refresh requests)
    Ok(raw_token)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_config() -> Config {
        Config {
            database_url: "postgres://localhost/test".to_string(),
            solana_rpc_url: "http://localhost:8899".to_string(),
            solana_ws_url: "ws://localhost:8900".to_string(),
            program_id: "11111111111111111111111111111111".to_string(),
            jwt_secret: "test_secret_that_is_long_enough_for_jwt".to_string(),
            jwt_access_expiry_seconds: 900,
            jwt_refresh_expiry_seconds: 604800,
            encryption_key: "test_encryption_key_long_enough_for_aes".to_string(),
            encryption_index_key: "test_index_key".to_string(),
            google_client_id: "test".to_string(),
            google_client_secret: "test".to_string(),
            google_redirect_uri: "http://localhost/callback".to_string(),
            microsoft_client_id: "test".to_string(),
            microsoft_client_secret: "test".to_string(),
            microsoft_redirect_uri: "http://localhost/callback".to_string(),
            microsoft_tenant: "common".to_string(),
            resend_api_key: "test".to_string(),
            email_from: "test@example.com".to_string(),
            invite_base_url: "http://localhost".to_string(),
            invite_expiry_seconds: 604800,
            invite_reminder_after_seconds: 259200,
            platform_fee_usd_cents: 199,
            platform_fee_free_tier: 3,
            platform_nonrefundable_fee_cents: 10,
            platform_vault_pubkey: "test".to_string(),
            platform_vault_keypair_path: std::path::PathBuf::from("/tmp/test"),
            platform_treasury_pubkey: "test".to_string(),
            platform_treasury_keypair_path: std::path::PathBuf::from("/tmp/test"),
            vault_min_sol_alert: 0.5,
            vault_min_sol_circuit_breaker: 0.1,
            vault_funding_rate_limit_per_hour: 50,
            treasury_min_usdc_alert: 20000000,
            treasury_float_per_token: 50000000,
            treasury_sweep_dest: "test".to_string(),
            stablecoin_registry: crate::config::StablecoinRegistry {
                usdc: crate::config::StablecoinInfo {
                    symbol: "usdc",
                    mint: "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v".to_string(),
                    ata: "test".to_string(),
                    decimals: 6,
                },
                usdt: crate::config::StablecoinInfo {
                    symbol: "usdt",
                    mint: "Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB".to_string(),
                    ata: "test".to_string(),
                    decimals: 6,
                },
                pyusd: crate::config::StablecoinInfo {
                    symbol: "pyusd",
                    mint: "2b1kV6DkPAnxd5ixfnxCpjxmKwqjjaYmCZfHsFu24GXo".to_string(),
                    ata: "test".to_string(),
                    decimals: 6,
                },
            },
            ipfs_api_url: "http://localhost".to_string(),
            ipfs_jwt: "test".to_string(),
            arweave_wallet_path: std::path::PathBuf::from("/tmp/test"),
            server_port: 8080,
            server_host: "localhost".to_string(),
        }
    }

    #[test]
    fn test_sha256_hex_produces_correct_hash() {
        let input = "test_refresh_token_value";
        let hash = sha256_hex(input);

        // Verify hash is 64 characters (256 bits in hex)
        assert_eq!(hash.len(), 64);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));

        // Verify deterministic (same input = same output)
        let hash2 = sha256_hex(input);
        assert_eq!(hash, hash2);

        // Verify different input produces different hash
        let hash3 = sha256_hex("different_input");
        assert_ne!(hash, hash3);
    }

    #[test]
    fn test_issue_and_decode_access_token_roundtrip() {
        let config = create_test_config();
        let user_id = Uuid::new_v4();
        let pubkey = Some("5HpHagT65TZzG1PH3CSu63k8DbpvD8s5ip6w3MubKHk".to_string());

        // Issue token
        let token = issue_access_token(user_id, pubkey.clone(), &config)
            .expect("Failed to issue access token");
        assert!(!token.is_empty());

        // Decode token
        let claims = decode_access_token(&token, &config).expect("Failed to decode access token");

        // Verify claims match
        assert_eq!(claims.sub, user_id);
        assert_eq!(claims.pubkey, pubkey);
        assert!(claims.exp > claims.iat);
        assert_eq!(claims.exp - claims.iat, config.jwt_access_expiry_seconds as usize);
    }

    #[test]
    fn test_issue_access_token_without_pubkey() {
        let config = create_test_config();
        let user_id = Uuid::new_v4();

        let token = issue_access_token(user_id, None, &config)
            .expect("Failed to issue access token");

        let claims = decode_access_token(&token, &config).expect("Failed to decode");
        assert_eq!(claims.sub, user_id);
        assert_eq!(claims.pubkey, None);
    }

    #[test]
    fn test_decode_expired_token_returns_unauthorized() {
        let config = create_test_config();
        let user_id = Uuid::new_v4();

        // Create a claim that's already expired
        let now = current_timestamp();
        let expired_claims = Claims {
            sub: user_id,
            pubkey: None,
            exp: now - 1, // 1 second in the past
            iat: now - 100,
            jti: Uuid::new_v4(),
        };

        // Manually encode it to bypass the issue_access_token function
        let encoding_key = EncodingKey::from_secret(config.jwt_secret.as_bytes());
        let token = encode(&Header::default(), &expired_claims, &encoding_key)
            .expect("Failed to encode expired token");

        // Decode should fail with Unauthorized
        let result = decode_access_token(&token, &config);
        assert!(matches!(result, Err(AppError::Unauthorized)));
    }

    #[test]
    fn test_decode_token_with_wrong_secret_returns_unauthorized() {
        let config = create_test_config();
        let user_id = Uuid::new_v4();

        // Issue token with correct secret
        let token = issue_access_token(user_id, None, &config)
            .expect("Failed to issue access token");

        // Create config with wrong secret
        let mut wrong_config = create_test_config();
        wrong_config.jwt_secret = "wrong_secret_that_is_long_enough".to_string();

        // Decode with wrong secret should fail
        let result = decode_access_token(&token, &wrong_config);
        assert!(matches!(result, Err(AppError::Unauthorized)));
    }

    #[test]
    fn test_issue_access_token_contains_unique_jti() {
        let config = create_test_config();
        let user_id = Uuid::new_v4();

        let token1 = issue_access_token(user_id, None, &config)
            .expect("Failed to issue access token");
        let token2 = issue_access_token(user_id, None, &config)
            .expect("Failed to issue access token");

        let claims1 = decode_access_token(&token1, &config).expect("Failed to decode");
        let claims2 = decode_access_token(&token2, &config).expect("Failed to decode");

        // Each token should have a unique jti
        assert_ne!(claims1.jti, claims2.jti);
    }

    #[test]
    fn test_sha256_hex_known_values() {
        // Test with known SHA-256 hashes
        let test_cases = vec![
            ("", "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"),
            (
                "test",
                "9f86d081884c7d6d9ffd60814e0406352b5914985847328e1640fc69ec282ca34",
            ),
        ];

        for (input, expected_hash) in test_cases {
            let hash = sha256_hex(input);
            assert_eq!(
                hash, expected_hash,
                "SHA-256 hash mismatch for input: {}",
                input
            );
        }
    }

    #[test]
    fn test_issue_access_token_respects_config_expiry() {
        let mut config = create_test_config();
        config.jwt_access_expiry_seconds = 1800; // 30 minutes

        let user_id = Uuid::new_v4();
        let token = issue_access_token(user_id, None, &config)
            .expect("Failed to issue access token");

        let claims = decode_access_token(&token, &config).expect("Failed to decode");
        assert_eq!(claims.exp - claims.iat, 1800);
    }
}
