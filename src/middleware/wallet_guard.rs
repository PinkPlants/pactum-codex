use crate::config::Config;
use crate::error::AppError;
use crate::middleware::auth::AuthUser;
use crate::state::AppState;
use async_trait::async_trait;
use axum::{
    extract::FromRequestParts,
    http::{request::Parts, StatusCode},
};
use uuid::Uuid;

/// Authenticated user with required wallet connection
/// Extracts AuthUser first, then validates pubkey is present
#[derive(Debug, Clone)]
pub struct AuthUserWithWallet {
    pub user_id: Uuid,
    pub pubkey: String, // guaranteed non-null — extractor rejects None
}

#[async_trait]
impl FromRequestParts<AppState> for AuthUserWithWallet
where
    AppState: Send + Sync,
{
    type Rejection = (StatusCode, String);

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        // First extract AuthUser (validates JWT)
        let auth = AuthUser::from_request_parts(parts, state)
            .await
            .map_err(|(status, msg)| (status, msg))?;

        // Then validate wallet is connected
        match auth.pubkey {
            Some(pubkey) => Ok(AuthUserWithWallet {
                user_id: auth.user_id,
                pubkey,
            }),
            None => {
                // Return WalletRequired error
                let error = AppError::WalletRequired {
                    message: "This action requires a connected wallet. Please link a wallet to your account.".to_string(),
                    link_url: "/auth/link/wallet".to_string(),
                };
                Err((StatusCode::FORBIDDEN, format!("{:?}", error)))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auth_user_with_wallet_struct() {
        let auth_user = AuthUserWithWallet {
            user_id: Uuid::new_v4(),
            pubkey: "test_pubkey".to_string(),
        };

        assert_eq!(auth_user.pubkey, "test_pubkey");
    }

    #[test]
    fn test_wallet_required_error_format() {
        let error = AppError::WalletRequired {
            message: "This action requires a connected wallet.".to_string(),
            link_url: "/auth/link/wallet".to_string(),
        };

        let formatted = format!("{:?}", error);
        assert!(formatted.contains("WalletRequired"));
        assert!(formatted.contains("/auth/link/wallet"));
    }
}
