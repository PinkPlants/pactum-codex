use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use thiserror::Error;

/// Complete error enum for Pactum API
#[derive(Debug, Error)]
pub enum AppError {
    // Authentication & Authorization (401/403)
    #[error("Invalid or expired nonce")]
    InvalidOrExpiredNonce,

    #[error("Invalid or expired refresh token")]
    InvalidRefreshToken,

    #[error("Unauthorized")]
    Unauthorized,

    // Wallet & Account (403/409/422)
    #[error("Wallet required")]
    WalletRequired { message: String, link_url: String },

    #[error("Email already registered")]
    EmailAlreadyRegistered,

    #[error("Email required")]
    EmailRequired {
        message: String,
        add_email_url: String,
    },

    #[error("Wallet already linked")]
    WalletAlreadyLinked,

    // File Upload Errors (400/422)
    #[error("Missing content type header")]
    MissingContentType,

    #[error("Invalid file type")]
    InvalidFileType,

    #[error("File too large")]
    FileTooLarge,

    #[error("Upload failed")]
    UploadFailed,

    // Draft & Payment (400/409/422)
    #[error("Draft not ready")]
    DraftNotReady,

    #[error("Payment required")]
    PaymentRequired {
        draft_id: String,
        initiate_url: String,
    },

    // Crypto & Hashing (400/422)
    #[error("Invalid hash")]
    InvalidHash,

    #[error("Hash mismatch")]
    HashMismatch,

    #[error("Encryption failed")]
    EncryptionFailed,

    #[error("Decryption failed")]
    DecryptionFailed,

    #[error("Keypair load failed: {0}")]
    KeypairLoadFailed(String),

    // Solana/Treasury (400/422)
    #[error("Payment method unsupported")]
    PaymentMethodUnsupported,

    #[error("Treasury ATA mismatch")]
    TreasuryAtaMismatch,

    #[error("No refund amount set")]
    NoRefundAmountSet,

    #[error("Vault deposit exceeds maximum")]
    VaultDepositExceedsMaximum,

    #[error("Invite window exceeds signing window")]
    InviteWindowExceedsSigningWindow,

    // Display Name (400/422)
    #[error("Display name too long")]
    DisplayNameTooLong,

    #[error("Invalid display name")]
    InvalidDisplayName,

    // Standard HTTP (404/500)
    #[error("Not found")]
    NotFound,

    #[error("Internal server error")]
    InternalError,

    #[error("Rate limited")]
    RateLimited,

    #[error("Not implemented")]
    NotImplemented,

    #[error("Solana RPC error")]
    SolanaRpcError,

    #[error("Transaction signing failed")]
    TransactionSigningFailed,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message, body) = match self {
            // 401 Unauthorized
            AppError::InvalidOrExpiredNonce => (
                StatusCode::UNAUTHORIZED,
                "Invalid or expired nonce",
                json!({ "error": "invalid_nonce" }),
            ),
            AppError::InvalidRefreshToken => (
                StatusCode::UNAUTHORIZED,
                "Invalid or expired refresh token",
                json!({ "error": "invalid_refresh_token" }),
            ),
            AppError::Unauthorized => (
                StatusCode::UNAUTHORIZED,
                "Unauthorized",
                json!({ "error": "unauthorized" }),
            ),

            // 403 Forbidden
            AppError::WalletRequired { message, link_url } => (
                StatusCode::FORBIDDEN,
                "Wallet required",
                json!({
                    "error": "wallet_required",
                    "message": message,
                    "link_url": link_url
                }),
            ),

            // 404 Not Found
            AppError::NotFound => (
                StatusCode::NOT_FOUND,
                "Not found",
                json!({ "error": "not_found" }),
            ),

            // 409 Conflict
            AppError::EmailAlreadyRegistered => (
                StatusCode::CONFLICT,
                "Email already registered",
                json!({ "error": "email_already_registered" }),
            ),

            AppError::WalletAlreadyLinked => (
                StatusCode::CONFLICT,
                "Wallet already linked",
                json!({ "error": "wallet_already_linked" }),
            ),

            // 422 Unprocessable Entity
            AppError::MissingContentType => (
                StatusCode::UNPROCESSABLE_ENTITY,
                "Missing content type header",
                json!({ "error": "missing_content_type" }),
            ),
            AppError::InvalidFileType => (
                StatusCode::UNPROCESSABLE_ENTITY,
                "Invalid file type",
                json!({ "error": "invalid_file_type" }),
            ),
            AppError::FileTooLarge => (
                StatusCode::UNPROCESSABLE_ENTITY,
                "File too large",
                json!({ "error": "file_too_large" }),
            ),
            AppError::UploadFailed => (
                StatusCode::UNPROCESSABLE_ENTITY,
                "Upload failed",
                json!({ "error": "upload_failed" }),
            ),
            AppError::DraftNotReady => (
                StatusCode::UNPROCESSABLE_ENTITY,
                "Draft not ready",
                json!({ "error": "draft_not_ready" }),
            ),
            AppError::PaymentRequired {
                draft_id,
                initiate_url,
            } => (
                StatusCode::UNPROCESSABLE_ENTITY,
                "Payment required",
                json!({
                    "error": "payment_required",
                    "draft_id": draft_id,
                    "initiate_url": initiate_url
                }),
            ),
            AppError::EmailRequired {
                message,
                add_email_url,
            } => (
                StatusCode::UNPROCESSABLE_ENTITY,
                "Email required",
                json!({
                    "error": "email_required",
                    "message": message,
                    "add_email_url": add_email_url
                }),
            ),
            AppError::InvalidHash => (
                StatusCode::UNPROCESSABLE_ENTITY,
                "Invalid hash",
                json!({ "error": "invalid_hash" }),
            ),
            AppError::HashMismatch => (
                StatusCode::BAD_REQUEST,
                "Hash mismatch",
                json!({ "error": "hash_mismatch" }),
            ),
            AppError::EncryptionFailed => (
                StatusCode::UNPROCESSABLE_ENTITY,
                "Encryption failed",
                json!({ "error": "encryption_failed" }),
            ),
            AppError::DecryptionFailed => (
                StatusCode::UNPROCESSABLE_ENTITY,
                "Decryption failed",
                json!({ "error": "decryption_failed" }),
            ),
            AppError::KeypairLoadFailed(msg) => (
                StatusCode::UNPROCESSABLE_ENTITY,
                "Keypair load failed",
                json!({ "error": "keypair_load_failed", "details": msg }),
            ),
            AppError::PaymentMethodUnsupported => (
                StatusCode::UNPROCESSABLE_ENTITY,
                "Payment method unsupported",
                json!({ "error": "payment_method_unsupported" }),
            ),
            AppError::TreasuryAtaMismatch => (
                StatusCode::UNPROCESSABLE_ENTITY,
                "Treasury ATA mismatch",
                json!({ "error": "treasury_ata_mismatch" }),
            ),
            AppError::NoRefundAmountSet => (
                StatusCode::UNPROCESSABLE_ENTITY,
                "No refund amount set",
                json!({ "error": "no_refund_amount_set" }),
            ),
            AppError::VaultDepositExceedsMaximum => (
                StatusCode::UNPROCESSABLE_ENTITY,
                "Vault deposit exceeds maximum",
                json!({ "error": "vault_deposit_exceeds_maximum" }),
            ),
            AppError::InviteWindowExceedsSigningWindow => (
                StatusCode::UNPROCESSABLE_ENTITY,
                "Invite window exceeds signing window",
                json!({ "error": "invite_window_exceeds_signing_window" }),
            ),
            AppError::DisplayNameTooLong => (
                StatusCode::UNPROCESSABLE_ENTITY,
                "Display name too long",
                json!({ "error": "display_name_too_long" }),
            ),
            AppError::InvalidDisplayName => (
                StatusCode::UNPROCESSABLE_ENTITY,
                "Invalid display name",
                json!({ "error": "invalid_display_name" }),
            ),

            // 429 Too Many Requests
            AppError::RateLimited => (
                StatusCode::TOO_MANY_REQUESTS,
                "Rate limited",
                json!({ "error": "rate_limited" }),
            ),

            // 500 Internal Server Error
            AppError::InternalError => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Internal server error",
                json!({ "error": "internal_error" }),
            ),

            // 501 Not Implemented
            AppError::SolanaRpcError => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Solana RPC error",
                json!({ "error": "solana_rpc_error" }),
            ),
            AppError::TransactionSigningFailed => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Transaction signing failed",
                json!({ "error": "transaction_signing_failed" }),
            ),

            AppError::NotImplemented => (
                StatusCode::NOT_IMPLEMENTED,
                "Not implemented",
                json!({ "error": "not_implemented" }),
            ),
        };

        tracing::error!("{}: {}", status.as_u16(), message);
        (status, Json(body)).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_invalid_or_expired_nonce_returns_401() {
        let err = AppError::InvalidOrExpiredNonce;
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn test_invalid_refresh_token_returns_401() {
        let err = AppError::InvalidRefreshToken;
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn test_unauthorized_returns_401() {
        let err = AppError::Unauthorized;
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn test_wallet_required_returns_403() {
        let err = AppError::WalletRequired {
            message: "Link a wallet".to_string(),
            link_url: "/auth/link/wallet".to_string(),
        };
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[test]
    fn test_wallet_required_includes_structured_fields() {
        let err = AppError::WalletRequired {
            message: "Link a wallet".to_string(),
            link_url: "/auth/link/wallet".to_string(),
        };
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        // Body is included in response
    }

    #[test]
    fn test_email_already_registered_returns_409() {
        let err = AppError::EmailAlreadyRegistered;
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::CONFLICT);
    }

    #[test]
    fn test_missing_content_type_returns_422() {
        let err = AppError::MissingContentType;
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[test]
    fn test_invalid_file_type_returns_422() {
        let err = AppError::InvalidFileType;
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[test]
    fn test_file_too_large_returns_422() {
        let err = AppError::FileTooLarge;
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[test]
    fn test_upload_failed_returns_422() {
        let err = AppError::UploadFailed;
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[test]
    fn test_draft_not_ready_returns_422() {
        let err = AppError::DraftNotReady;
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[test]
    fn test_payment_required_returns_422() {
        let err = AppError::PaymentRequired {
            draft_id: "draft-123".to_string(),
            initiate_url: "/payment/initiate/draft-123".to_string(),
        };
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[test]
    fn test_payment_required_includes_structured_fields() {
        let err = AppError::PaymentRequired {
            draft_id: "draft-123".to_string(),
            initiate_url: "/payment/initiate/draft-123".to_string(),
        };
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        // Body includes draft_id and initiate_url
    }

    #[test]
    fn test_email_required_returns_422() {
        let err = AppError::EmailRequired {
            message: "Add email".to_string(),
            add_email_url: "/user/contacts".to_string(),
        };
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[test]
    fn test_invalid_hash_returns_422() {
        let err = AppError::InvalidHash;
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[test]
    fn test_hash_mismatch_returns_400() {
        let err = AppError::HashMismatch;
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn test_encryption_failed_returns_422() {
        let err = AppError::EncryptionFailed;
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[test]
    fn test_decryption_failed_returns_422() {
        let err = AppError::DecryptionFailed;
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[test]
    fn test_keypair_load_failed_returns_422() {
        let err = AppError::KeypairLoadFailed("file not found".to_string());
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[test]
    fn test_payment_method_unsupported_returns_422() {
        let err = AppError::PaymentMethodUnsupported;
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[test]
    fn test_treasury_ata_mismatch_returns_422() {
        let err = AppError::TreasuryAtaMismatch;
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[test]
    fn test_no_refund_amount_set_returns_422() {
        let err = AppError::NoRefundAmountSet;
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[test]
    fn test_vault_deposit_exceeds_maximum_returns_422() {
        let err = AppError::VaultDepositExceedsMaximum;
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[test]
    fn test_invite_window_exceeds_signing_window_returns_422() {
        let err = AppError::InviteWindowExceedsSigningWindow;
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[test]
    fn test_display_name_too_long_returns_422() {
        let err = AppError::DisplayNameTooLong;
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[test]
    fn test_invalid_display_name_returns_422() {
        let err = AppError::InvalidDisplayName;
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[test]
    fn test_not_found_returns_404() {
        let err = AppError::NotFound;
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn test_rate_limited_returns_429() {
        let err = AppError::RateLimited;
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    }

    #[test]
    fn test_internal_error_returns_500() {
        let err = AppError::InternalError;
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }
}
