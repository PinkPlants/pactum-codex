use crate::config::Config;
use crate::error::AppError;
use crate::middleware::auth::AuthUser;
use crate::services::crypto::{encrypt, hmac_index};
use axum::{extract::State, Json};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

// ===== REQUEST/RESPONSE TYPES =====

#[derive(Debug, Serialize)]
pub struct UserProfile {
    pub id: Uuid,
    pub display_name: Option<String>,
    pub linked_auth_methods: Vec<AuthMethod>,
}

#[derive(Debug, Serialize)]
pub struct AuthMethod {
    #[serde(rename = "type")]
    pub auth_type: String,
    pub provider: Option<String>,
    pub pubkey: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateProfileRequest {
    pub display_name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateContactsRequest {
    pub email: Option<String>,
    pub phone: Option<String>,
    pub push_token: Option<String>,
}

// ===== HANDLERS =====

/// GET /user/me
/// Get current user profile with linked auth methods
pub async fn get_profile(
    State(db): State<PgPool>,
    auth: AuthUser,
) -> Result<Json<UserProfile>, AppError> {
    // Get user info
    let user = sqlx::query_as::<_, (Uuid, Option<String>)>(
        "SELECT id, display_name FROM user_accounts WHERE id = $1",
    )
    .bind(auth.user_id)
    .fetch_one(&db)
    .await
    .map_err(|_| AppError::InternalError)?;

    let mut linked_methods = Vec::new();

    // Check for wallet auth
    if let Some(wallet_row) =
        sqlx::query_scalar::<_, String>("SELECT pubkey FROM auth_wallet WHERE user_id = $1")
            .bind(auth.user_id)
            .fetch_optional(&db)
            .await
            .map_err(|_| AppError::InternalError)?
    {
        linked_methods.push(AuthMethod {
            auth_type: "wallet".to_string(),
            provider: None,
            pubkey: Some(wallet_row),
        });
    }

    // Check for OAuth methods
    let oauth_rows =
        sqlx::query_scalar::<_, String>("SELECT provider FROM auth_oauth WHERE user_id = $1")
            .bind(auth.user_id)
            .fetch_all(&db)
            .await
            .map_err(|_| AppError::InternalError)?;

    for oauth in oauth_rows {
        linked_methods.push(AuthMethod {
            auth_type: "oauth".to_string(),
            provider: Some(oauth),
            pubkey: None,
        });
    }

    Ok(Json(UserProfile {
        id: user.0,
        display_name: user.1,
        linked_auth_methods: linked_methods,
    }))
}

/// PUT /user/me
/// Update user profile (display name)
pub async fn update_profile(
    State(db): State<PgPool>,
    auth: AuthUser,
    Json(body): Json<UpdateProfileRequest>,
) -> Result<Json<UserProfile>, AppError> {
    // Validate display name if provided
    if let Some(ref name) = body.display_name {
        sanitise_display_name(name)?;
    }

    // Update user
    sqlx::query("UPDATE user_accounts SET display_name = $1 WHERE id = $2")
        .bind(&body.display_name)
        .bind(auth.user_id)
        .execute(&db)
        .await
        .map_err(|_| AppError::InternalError)?;

    // Return updated profile
    get_profile(State(db), auth).await
}

/// PUT /user/contacts
/// Update user contact information (encrypted)
pub async fn update_contacts(
    State(db): State<PgPool>,
    State(config): State<Config>,
    auth: AuthUser,
    Json(body): Json<UpdateContactsRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let key = decode_hex_key_32(&config.encryption_key)?;
    let idx_key = decode_hex_key_32(&config.encryption_index_key)?;

    // Encrypt email if provided
    let (email_enc, email_nonce, email_index) = if let Some(ref email) = body.email {
        let (ciphertext, nonce) = encrypt(email, &key)?;
        let index = hmac_index(email, &idx_key);
        (Some(ciphertext), Some(nonce.to_vec()), Some(index))
    } else {
        (None, None, None)
    };

    // Encrypt phone if provided
    let (phone_enc, phone_nonce) = if let Some(ref phone) = body.phone {
        let (ciphertext, nonce) = encrypt(phone, &key)?;
        (Some(ciphertext), Some(nonce.to_vec()))
    } else {
        (None, None)
    };

    // Encrypt push_token if provided
    let (push_enc, push_nonce) = if let Some(ref token) = body.push_token {
        let (ciphertext, nonce) = encrypt(token, &key)?;
        (Some(ciphertext), Some(nonce.to_vec()))
    } else {
        (None, None)
    };

    // UPSERT contacts
    sqlx::query(
        "INSERT INTO user_contacts (user_id, email_enc, email_nonce, email_index, phone_enc, phone_nonce, push_token_enc, push_token_nonce, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, extract(epoch from now()))
         ON CONFLICT (user_id)
         DO UPDATE SET
             email_enc = COALESCE($2, user_contacts.email_enc),
             email_nonce = COALESCE($3, user_contacts.email_nonce),
             email_index = COALESCE($4, user_contacts.email_index),
             phone_enc = COALESCE($5, user_contacts.phone_enc),
             phone_nonce = COALESCE($6, user_contacts.phone_nonce),
             push_token_enc = COALESCE($7, user_contacts.push_token_enc),
             push_token_nonce = COALESCE($8, user_contacts.push_token_nonce),
             updated_at = extract(epoch from now())",
    )
    .bind(auth.user_id)
    .bind(email_enc)
    .bind(email_nonce)
    .bind(email_index)
    .bind(phone_enc)
    .bind(phone_nonce)
    .bind(push_enc)
    .bind(push_nonce)
    .execute(&db)
    .await
    .map_err(|_| AppError::InternalError)?;

    Ok(Json(
        serde_json::json!({ "message": "Contacts updated successfully" }),
    ))
}

/// DELETE /user/contacts
/// Delete user contact information
pub async fn delete_contacts(
    State(db): State<PgPool>,
    auth: AuthUser,
) -> Result<Json<serde_json::Value>, AppError> {
    sqlx::query("DELETE FROM user_contacts WHERE user_id = $1")
        .bind(auth.user_id)
        .execute(&db)
        .await
        .map_err(|_| AppError::InternalError)?;

    Ok(Json(
        serde_json::json!({ "message": "Contacts deleted successfully" }),
    ))
}

// ===== HELPERS =====

/// Sanitise display name per spec §8.5
/// Rejects if >64 chars or contains HTML chars (< > " ' &)
fn sanitise_display_name(name: &str) -> Result<(), AppError> {
    if name.len() > 64 {
        return Err(AppError::DisplayNameTooLong);
    }

    // Reject HTML chars to prevent XSS
    if name.contains('<')
        || name.contains('>')
        || name.contains('"')
        || name.contains('\'')
        || name.contains('&')
    {
        return Err(AppError::InvalidDisplayName);
    }

    Ok(())
}

fn decode_hex_key_32(key_hex: &str) -> Result<[u8; 32], AppError> {
    let key_bytes = hex::decode(key_hex).map_err(|_| AppError::InternalError)?;
    key_bytes.try_into().map_err(|_| AppError::InternalError)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitise_display_name_valid() {
        assert!(sanitise_display_name("Alice").is_ok());
        assert!(sanitise_display_name("Bob Smith").is_ok());
        assert!(sanitise_display_name("User-123").is_ok());
    }

    #[test]
    fn test_sanitise_display_name_too_long() {
        let long_name = "a".repeat(65);
        assert!(matches!(
            sanitise_display_name(&long_name),
            Err(AppError::DisplayNameTooLong)
        ));
    }

    #[test]
    fn test_sanitise_display_name_html_chars() {
        assert!(matches!(
            sanitise_display_name("<script>alert('xss')</script>"),
            Err(AppError::InvalidDisplayName)
        ));
        assert!(matches!(
            sanitise_display_name("Name>Malicious"),
            Err(AppError::InvalidDisplayName)
        ));
        assert!(matches!(
            sanitise_display_name("Name\"Quote"),
            Err(AppError::InvalidDisplayName)
        ));
    }

    #[test]
    fn test_user_profile_structure() {
        let profile = UserProfile {
            id: Uuid::new_v4(),
            display_name: Some("Test User".to_string()),
            linked_auth_methods: vec![AuthMethod {
                auth_type: "wallet".to_string(),
                provider: None,
                pubkey: Some("test_pubkey".to_string()),
            }],
        };

        assert_eq!(profile.linked_auth_methods.len(), 1);
        assert_eq!(profile.linked_auth_methods[0].auth_type, "wallet");
    }
}
