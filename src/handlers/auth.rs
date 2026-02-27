use crate::config::Config;
use crate::error::AppError;
use crate::services::jwt::{issue_access_token, issue_and_store_refresh_token};
use axum::{extract::State, Json};
use serde::{Deserialize, Serialize};
use solana_sdk::{pubkey::Pubkey, signature::Signature};
use sqlx::PgPool;
use std::str::FromStr;
use uuid::Uuid;

// ===== REQUEST/RESPONSE TYPES =====

#[derive(Debug, Serialize)]
pub struct ChallengeResponse {
    pub nonce: String,
}

#[derive(Debug, Deserialize)]
pub struct VerifyRequest {
    pub pubkey: String,
    pub signature: String, // base64-encoded
    pub nonce: String,
}

#[derive(Debug, Serialize)]
pub struct AuthResponse {
    pub access_token: String,
    pub refresh_token: String,
}

#[derive(Debug, Deserialize)]
pub struct RefreshRequest {
    pub refresh_token: String,
}

// ===== SIWS HANDLERS =====

/// GET /auth/challenge
/// Generate a UUID nonce and store in siws_nonces table
pub async fn challenge(State(db): State<PgPool>) -> Result<Json<ChallengeResponse>, AppError> {
    let nonce = Uuid::new_v4().to_string();

    sqlx::query("INSERT INTO siws_nonces (nonce) VALUES ($1)")
        .bind(&nonce)
        .execute(&db)
        .await
        .map_err(|_| AppError::InternalError)?;

    Ok(Json(ChallengeResponse { nonce }))
}

/// POST /auth/verify
/// Verify SIWS signature and issue access/refresh tokens
pub async fn verify(
    State(db): State<PgPool>,
    State(config): State<Config>,
    Json(body): Json<VerifyRequest>,
) -> Result<Json<AuthResponse>, AppError> {
    // 1. Atomically consume nonce (prevents replay attacks)
    let row = sqlx::query!(
        "DELETE FROM siws_nonces
         WHERE nonce = $1
           AND created_at > extract(epoch from now()) - 300
         RETURNING nonce",
        body.nonce
    )
    .fetch_optional(&db)
    .await
    .map_err(|_| AppError::InternalError)?;

    if row.is_none() {
        return Err(AppError::InvalidOrExpiredNonce);
    }

    // 2. Verify ed25519 signature
    let pubkey = Pubkey::from_str(&body.pubkey).map_err(|_| AppError::InvalidSignature)?;

    let signature =
        Signature::from_str(&body.signature).map_err(|_| AppError::InvalidSignature)?;

    let nonce_bytes = body.nonce.as_bytes();

    // Verify signature
    if !signature.verify(pubkey.as_ref(), nonce_bytes) {
        return Err(AppError::InvalidSignature);
    }

    // 3. Upsert user_accounts + auth_wallet (implicit signup)
    let user_id = sqlx::query_scalar!(
        "INSERT INTO user_accounts (pubkey, created_at, updated_at)
         VALUES ($1, extract(epoch from now()), extract(epoch from now()))
         ON CONFLICT (pubkey)
         DO UPDATE SET updated_at = extract(epoch from now())
         RETURNING id",
        body.pubkey
    )
    .fetch_one(&db)
    .await
    .map_err(|_| AppError::InternalError)?;

    // Upsert auth_wallet
    sqlx::query!(
        "INSERT INTO auth_wallet (user_id, pubkey, created_at)
         VALUES ($1, $2, extract(epoch from now()))
         ON CONFLICT (user_id)
         DO UPDATE SET pubkey = $2, updated_at = extract(epoch from now())",
        user_id,
        body.pubkey
    )
    .execute(&db)
    .await
    .map_err(|_| AppError::InternalError)?;

    // 4. Issue tokens
    let access_token = issue_access_token(user_id, Some(body.pubkey), &config)?;
    let refresh_token = issue_and_store_refresh_token(&db, user_id).await?;

    Ok(Json(AuthResponse {
        access_token,
        refresh_token,
    }))
}

// ===== REFRESH & LOGOUT =====

/// POST /auth/refresh
/// Rotate refresh token and issue new access token
pub async fn refresh(
    State(db): State<PgPool>,
    State(config): State<Config>,
    Json(body): Json<RefreshRequest>,
) -> Result<Json<AuthResponse>, AppError> {
    use crate::services::jwt::sha256_hex;

    // Hash the incoming refresh token
    let token_hash = sha256_hex(&body.refresh_token);

    // Delete-on-use: atomically consume refresh token
    let row = sqlx::query!(
        "DELETE FROM refresh_tokens
         WHERE token_hash = $1
           AND expires_at > extract(epoch from now())
         RETURNING user_id",
        token_hash
    )
    .fetch_optional(&db)
    .await
    .map_err(|_| AppError::InternalError)?;

    let user_id = row
        .ok_or(AppError::InvalidRefreshToken)?
        .user_id;

    // Fetch user's current wallet pubkey (may have changed)
    let pubkey = sqlx::query_scalar!(
        "SELECT pubkey FROM auth_wallet WHERE user_id = $1",
        user_id
    )
    .fetch_optional(&db)
    .await
    .map_err(|_| AppError::InternalError)?;

    // Issue new tokens
    let access_token = issue_access_token(user_id, pubkey.clone(), &config)?;
    let refresh_token = issue_and_store_refresh_token(&db, user_id).await?;

    Ok(Json(AuthResponse {
        access_token,
        refresh_token,
    }))
}

/// POST /auth/logout
/// Delete refresh token to invalidate session
pub async fn logout(
    State(db): State<PgPool>,
    Json(body): Json<RefreshRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    use crate::services::jwt::sha256_hex;

    let token_hash = sha256_hex(&body.refresh_token);

    sqlx::query!("DELETE FROM refresh_tokens WHERE token_hash = $1", token_hash)
        .execute(&db)
        .await
        .map_err(|_| AppError::InternalError)?;

    Ok(Json(serde_json::json!({ "message": "Logged out successfully" })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_challenge_response_structure() {
        let response = ChallengeResponse {
            nonce: Uuid::new_v4().to_string(),
        };
        assert!(!response.nonce.is_empty());
    }

    #[test]
    fn test_verify_request_structure() {
        let request = VerifyRequest {
            pubkey: "test_pubkey".to_string(),
            signature: "test_signature".to_string(),
            nonce: Uuid::new_v4().to_string(),
        };
        assert!(!request.nonce.is_empty());
    }

    #[test]
    fn test_auth_response_structure() {
        let response = AuthResponse {
            access_token: "test_access".to_string(),
            refresh_token: "test_refresh".to_string(),
        };
        assert!(!response.access_token.is_empty());
        assert!(!response.refresh_token.is_empty());
    }
}
