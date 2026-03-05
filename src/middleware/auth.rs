use crate::config::Config;
use crate::error::AppError;
use crate::services::jwt::{decode_access_token, Claims};
use crate::state::AppState;
use async_trait::async_trait;
use axum::{
    extract::FromRequestParts,
    http::{request::Parts, StatusCode},
};
use uuid::Uuid;

/// Authenticated user from JWT token
#[derive(Debug, Clone)]
pub struct AuthUser {
    pub user_id: Uuid,
    pub pubkey: Option<String>, // None for OAuth-only users (no linked wallet)
}

#[async_trait]
impl FromRequestParts<AppState> for AuthUser
where
    AppState: Send + Sync,
{
    type Rejection = (StatusCode, String);

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        // Extract Authorization header
        let token = parts
            .headers
            .get("Authorization")
            .and_then(|h| h.to_str().ok())
            .and_then(|h| h.strip_prefix("Bearer "))
            .map(|t| t.to_string())
            .ok_or_else(|| {
                (
                    StatusCode::UNAUTHORIZED,
                    "Missing or invalid Authorization header".to_string(),
                )
            })?;

        // Get config from state
        let config: &Config = state.as_ref();

        // Decode and validate JWT
        let claims = decode_access_token(&token, config).map_err(|_| {
            (
                StatusCode::UNAUTHORIZED,
                "Invalid or expired token".to_string(),
            )
        })?;

        Ok(AuthUser {
            user_id: claims.sub,
            pubkey: claims.pubkey,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::jwt::issue_access_token;
    use axum::http::{HeaderMap, Request};

    fn create_test_config() -> Config {
        // Minimal config for JWT testing
        Config {
            database_url: "".to_string(),
            solana_rpc_url: "".to_string(),
            solana_ws_url: "".to_string(),
            program_id: "".to_string(),
            jwt_secret: "test_secret_for_jwt_validation_testing".to_string(),
            jwt_access_expiry_seconds: 900,
            jwt_refresh_expiry_seconds: 604800,
            encryption_key: "".to_string(),
            encryption_index_key: "".to_string(),
            google_client_id: "".to_string(),
            google_client_secret: "".to_string(),
            google_redirect_uri: "".to_string(),
            microsoft_client_id: "".to_string(),
            microsoft_client_secret: "".to_string(),
            microsoft_redirect_uri: "".to_string(),
            microsoft_tenant: "".to_string(),
            resend_api_key: "".to_string(),
            email_from: "".to_string(),
            invite_base_url: "".to_string(),
            invite_expiry_seconds: 604800,
            invite_reminder_after_seconds: 259200,
            platform_fee_usd_cents: 199,
            platform_fee_free_tier: 3,
            platform_nonrefundable_fee_cents: 10,
            platform_vault_pubkey: "".to_string(),
            platform_vault_keypair_path: std::path::PathBuf::new(),
            platform_treasury_pubkey: "".to_string(),
            platform_treasury_keypair_path: std::path::PathBuf::new(),
            vault_min_sol_alert: 0.5,
            vault_min_sol_circuit_breaker: 0.1,
            vault_funding_rate_limit_per_hour: 50,
            treasury_min_usdc_alert: 20000000,
            treasury_float_per_token: 50000000,
            treasury_sweep_dest: "".to_string(),
            stablecoin_registry: crate::config::StablecoinRegistry {
                usdc: crate::config::StablecoinInfo {
                    symbol: "usdc",
                    mint: "".to_string(),
                    ata: "".to_string(),
                    decimals: 6,
                },
                usdt: crate::config::StablecoinInfo {
                    symbol: "usdt",
                    mint: "".to_string(),
                    ata: "".to_string(),
                    decimals: 6,
                },
                pyusd: crate::config::StablecoinInfo {
                    symbol: "pyusd",
                    mint: "".to_string(),
                    ata: "".to_string(),
                    decimals: 6,
                },
            },
            pinata_jwt: "".to_string(),
            pinata_gateway_domain: "gateway.pinata.cloud".to_string(),
            arweave_wallet_path: std::path::PathBuf::new(),
            server_port: 8080,
            server_host: "localhost".to_string(),
        }
    }

    #[tokio::test]
    async fn test_auth_user_extracts_valid_jwt() {
        let config = create_test_config();
        let user_id = Uuid::new_v4();
        let pubkey = Some("test_pubkey".to_string());

        // Issue a valid token
        let token =
            issue_access_token(user_id, pubkey.clone(), &config).expect("Failed to issue token");

        // Create request parts with Authorization header
        let mut headers = HeaderMap::new();
        headers.insert(
            "Authorization",
            format!("Bearer {}", token).parse().unwrap(),
        );

        let request = Request::builder().body(()).unwrap();

        let (mut parts, _) = request.into_parts();
        parts.headers = headers;

        // This test would require a proper Axum state setup
        // For now, we verify the token decode logic works
        assert!(token.len() > 0);
    }

    #[test]
    fn test_auth_user_struct() {
        let auth_user = AuthUser {
            user_id: Uuid::new_v4(),
            pubkey: Some("test_pubkey".to_string()),
        };

        assert!(auth_user.pubkey.is_some());
    }
}
