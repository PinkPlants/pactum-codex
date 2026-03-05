use crate::config::Config;
use crate::error::AppError;
use crate::services::crypto::{encrypt, hmac_index};
use crate::services::jwt::{
    decode_access_token, issue_access_token, issue_and_store_refresh_token,
};
use crate::state::AppState;
use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
    Json,
};
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use oauth2::{
    basic::BasicClient, AuthUrl, ClientId, ClientSecret, CsrfToken, RedirectUrl, Scope, TokenUrl,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use solana_sdk::{pubkey::Pubkey, signature::Signature};
use sqlx::PgPool;
use std::future::Future;
use std::pin::Pin;
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

#[derive(Debug, Deserialize)]
pub struct LinkWalletRequest {
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

#[derive(Debug, Deserialize)]
pub struct OAuthCallbackQuery {
    pub code: String,
    pub state: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OAuthProvider {
    Google,
    Microsoft,
}

impl OAuthProvider {
    fn as_str(self) -> &'static str {
        match self {
            Self::Google => "google",
            Self::Microsoft => "microsoft",
        }
    }

    fn auth_url(self, config: &Config) -> String {
        match self {
            Self::Google => "https://accounts.google.com/o/oauth2/v2/auth".to_string(),
            Self::Microsoft => format!(
                "https://login.microsoftonline.com/{}/oauth2/v2.0/authorize",
                config.microsoft_tenant
            ),
        }
    }

    fn token_url(self, config: &Config) -> String {
        match self {
            Self::Google => "https://oauth2.googleapis.com/token".to_string(),
            Self::Microsoft => format!(
                "https://login.microsoftonline.com/{}/oauth2/v2.0/token",
                config.microsoft_tenant
            ),
        }
    }

    fn profile_url(self) -> &'static str {
        match self {
            Self::Google => "https://openidconnect.googleapis.com/v1/userinfo",
            Self::Microsoft => "https://graph.microsoft.com/oidc/userinfo",
        }
    }

    fn client_id(self, config: &Config) -> &str {
        match self {
            Self::Google => &config.google_client_id,
            Self::Microsoft => &config.microsoft_client_id,
        }
    }

    fn client_secret(self, config: &Config) -> &str {
        match self {
            Self::Google => &config.google_client_secret,
            Self::Microsoft => &config.microsoft_client_secret,
        }
    }

    fn redirect_uri(self, config: &Config) -> &str {
        match self {
            Self::Google => &config.google_redirect_uri,
            Self::Microsoft => &config.microsoft_redirect_uri,
        }
    }

    fn scopes(self) -> [&'static str; 3] {
        ["openid", "email", "profile"]
    }

    fn state_cookie_name(self) -> &'static str {
        match self {
            Self::Google => "oauth_google_state",
            Self::Microsoft => "oauth_microsoft_state",
        }
    }
}

#[derive(Debug)]
struct OAuthIdentity {
    provider_id: String,
    email: String,
}

#[derive(Debug)]
struct EmailConflictDetails {
    existing_provider: String,
    link_url: String,
}

enum OAuthUserResolution {
    User(Uuid),
    Conflict(EmailConflictDetails),
}

trait OAuthHttpClient {
    fn post_form<'a>(
        &'a self,
        url: &'a str,
        form: Vec<(String, String)>,
    ) -> Pin<Box<dyn Future<Output = Result<Value, AppError>> + Send + 'a>>;

    fn get_json<'a>(
        &'a self,
        url: &'a str,
        bearer_token: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<Value, AppError>> + Send + 'a>>;
}

struct ReqwestOAuthHttpClient {
    inner: reqwest::Client,
}

impl ReqwestOAuthHttpClient {
    fn new() -> Self {
        Self {
            inner: reqwest::Client::new(),
        }
    }
}

impl OAuthHttpClient for ReqwestOAuthHttpClient {
    fn post_form<'a>(
        &'a self,
        url: &'a str,
        form: Vec<(String, String)>,
    ) -> Pin<Box<dyn Future<Output = Result<Value, AppError>> + Send + 'a>> {
        Box::pin(async move {
            let response = self
                .inner
                .post(url)
                .form(&form)
                .send()
                .await
                .map_err(|_| AppError::InternalError)?;

            if !response.status().is_success() {
                return Err(AppError::InternalError);
            }

            response
                .json::<Value>()
                .await
                .map_err(|_| AppError::InternalError)
        })
    }

    fn get_json<'a>(
        &'a self,
        url: &'a str,
        bearer_token: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<Value, AppError>> + Send + 'a>> {
        Box::pin(async move {
            let response = self
                .inner
                .get(url)
                .bearer_auth(bearer_token)
                .send()
                .await
                .map_err(|_| AppError::InternalError)?;

            if !response.status().is_success() {
                return Err(AppError::InternalError);
            }

            response
                .json::<Value>()
                .await
                .map_err(|_| AppError::InternalError)
        })
    }
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
    let pubkey_str = body.pubkey.clone();

    // 1. Atomically consume nonce (prevents replay attacks)
    let row = sqlx::query_scalar::<_, String>(
        "DELETE FROM siws_nonces
         WHERE nonce = $1
           AND created_at > extract(epoch from now()) - 300
         RETURNING nonce",
    )
    .bind(&body.nonce)
    .fetch_optional(&db)
    .await
    .map_err(|_| AppError::InternalError)?;

    if row.is_none() {
        return Err(AppError::InvalidOrExpiredNonce);
    }

    // 2. Verify ed25519 signature
    let pubkey = Pubkey::from_str(&body.pubkey).map_err(|_| AppError::Unauthorized)?;

    let signature = Signature::from_str(&body.signature).map_err(|_| AppError::Unauthorized)?;

    let nonce_bytes = body.nonce.as_bytes();

    // Verify signature
    if !signature.verify(pubkey.as_ref(), nonce_bytes) {
        return Err(AppError::Unauthorized);
    }

    let existing_user_id =
        sqlx::query_scalar::<_, Uuid>("SELECT user_id FROM auth_wallet WHERE pubkey = $1")
            .bind(&pubkey_str)
            .fetch_optional(&db)
            .await
            .map_err(|_| AppError::InternalError)?;

    let user_id = match existing_user_id {
        Some(user_id) => user_id,
        None => {
            let user_id = sqlx::query_scalar::<_, Uuid>(
                "INSERT INTO user_accounts DEFAULT VALUES RETURNING id",
            )
            .fetch_one(&db)
            .await
            .map_err(|_| AppError::InternalError)?;

            sqlx::query("INSERT INTO auth_wallet (user_id, pubkey) VALUES ($1, $2)")
                .bind(user_id)
                .bind(&body.pubkey)
                .execute(&db)
                .await
                .map_err(|_| AppError::InternalError)?;

            user_id
        }
    };

    // 4. Issue tokens
    let access_token = issue_access_token(user_id, Some(pubkey_str), &config)?;
    let refresh_token = issue_and_store_refresh_token(&db, user_id).await?;

    Ok(Json(AuthResponse {
        access_token,
        refresh_token,
    }))
}

/// POST /auth/link/wallet
/// Link a wallet pubkey to an OAuth-authenticated user
/// Requires valid OAuth JWT with pubkey=None
pub async fn link_wallet(
    State(db): State<PgPool>,
    State(config): State<Config>,
    auth: crate::middleware::auth::AuthUser,
    Json(body): Json<LinkWalletRequest>,
) -> Result<Json<AuthResponse>, AppError> {
    // Verify user is authenticated but has no wallet linked yet
    if auth.pubkey.is_some() {
        return Err(AppError::WalletAlreadyLinked);
    }

    let pubkey_str = body.pubkey.clone();

    // 1. Atomically consume nonce (same pattern as verify)
    let row = sqlx::query_scalar::<_, String>(
        "DELETE FROM siws_nonces
         WHERE nonce = $1
           AND created_at > extract(epoch from now()) - 300
         RETURNING nonce",
    )
    .bind(&body.nonce)
    .fetch_optional(&db)
    .await
    .map_err(|_| AppError::InternalError)?;

    if row.is_none() {
        return Err(AppError::InvalidOrExpiredNonce);
    }

    // 2. Verify ed25519 signature (same pattern as verify)
    let pubkey = Pubkey::from_str(&body.pubkey).map_err(|_| AppError::Unauthorized)?;

    let signature = Signature::from_str(&body.signature).map_err(|_| AppError::Unauthorized)?;

    let nonce_bytes = body.nonce.as_bytes();

    // Verify signature
    if !signature.verify(pubkey.as_ref(), nonce_bytes) {
        return Err(AppError::Unauthorized);
    }

    // 3. Check if pubkey is already linked to another user
    let existing_user_id =
        sqlx::query_scalar::<_, Uuid>("SELECT user_id FROM auth_wallet WHERE pubkey = $1")
            .bind(&pubkey_str)
            .fetch_optional(&db)
            .await
            .map_err(|_| AppError::InternalError)?;

    if existing_user_id.is_some() {
        // Pubkey already linked to another user
        return Err(AppError::WalletAlreadyLinked);
    }

    // 4. Insert wallet link for current user
    sqlx::query("INSERT INTO auth_wallet (user_id, pubkey) VALUES ($1, $2)")
        .bind(auth.user_id)
        .bind(&pubkey_str)
        .execute(&db)
        .await
        .map_err(|_| AppError::InternalError)?;

    // 5. Issue new JWT with pubkey included in claims
    let access_token = issue_access_token(auth.user_id, Some(pubkey_str), &config)?;
    let refresh_token = issue_and_store_refresh_token(&db, auth.user_id).await?;

    Ok(Json(AuthResponse {
        access_token,
        refresh_token,
    }))
}

pub async fn oauth_google(State(state): State<AppState>, jar: CookieJar) -> impl IntoResponse {
    oauth_redirect(state.config.as_ref(), jar, OAuthProvider::Google)
}

pub async fn oauth_microsoft(State(state): State<AppState>, jar: CookieJar) -> impl IntoResponse {
    oauth_redirect(state.config.as_ref(), jar, OAuthProvider::Microsoft)
}

pub async fn oauth_google_callback(
    State(state): State<AppState>,
    jar: CookieJar,
    Query(query): Query<OAuthCallbackQuery>,
) -> Result<Response, AppError> {
    oauth_callback(
        &state,
        jar,
        query,
        OAuthProvider::Google,
        &ReqwestOAuthHttpClient::new(),
    )
    .await
}

pub async fn oauth_microsoft_callback(
    State(state): State<AppState>,
    jar: CookieJar,
    Query(query): Query<OAuthCallbackQuery>,
) -> Result<Response, AppError> {
    oauth_callback(
        &state,
        jar,
        query,
        OAuthProvider::Microsoft,
        &ReqwestOAuthHttpClient::new(),
    )
    .await
}

fn oauth_redirect(config: &Config, jar: CookieJar, provider: OAuthProvider) -> impl IntoResponse {
    match build_provider_auth_url(config, provider) {
        Ok((auth_url, state)) => {
            let cookie = build_oauth_state_cookie(provider.state_cookie_name(), &state);
            (jar.add(cookie), Redirect::to(auth_url.as_str())).into_response()
        }
        Err(_) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "internal_error" })),
        )
            .into_response(),
    }
}

async fn oauth_callback<C: OAuthHttpClient>(
    state: &AppState,
    jar: CookieJar,
    query: OAuthCallbackQuery,
    provider: OAuthProvider,
    http: &C,
) -> Result<Response, AppError> {
    validate_oauth_state_cookie(&jar, provider, &query.state)?;

    let identity =
        exchange_code_and_fetch_identity(http, state.config.as_ref(), provider, &query.code)
            .await?;

    let resolution = resolve_or_create_oauth_user(
        &state.db,
        state.config.as_ref(),
        provider,
        &identity.provider_id,
        &identity.email,
    )
    .await?;

    let user_id = match resolution {
        OAuthUserResolution::User(user_id) => user_id,
        OAuthUserResolution::Conflict(conflict) => {
            return Ok((
                StatusCode::CONFLICT,
                Json(json!({
                    "error": "email_already_registered",
                    "existing_provider": conflict.existing_provider,
                    "link_url": conflict.link_url,
                })),
            )
                .into_response());
        }
    };

    let access_token = issue_access_token(user_id, None, state.config.as_ref())?;
    let refresh_token = issue_and_store_refresh_token(&state.db, user_id).await?;

    Ok(Json(AuthResponse {
        access_token,
        refresh_token,
    })
    .into_response())
}

fn build_provider_auth_url(
    config: &Config,
    provider: OAuthProvider,
) -> Result<(String, String), AppError> {
    let auth_url = AuthUrl::new(provider.auth_url(config)).map_err(|_| AppError::InternalError)?;
    let token_url =
        TokenUrl::new(provider.token_url(config)).map_err(|_| AppError::InternalError)?;
    let redirect_uri = RedirectUrl::new(provider.redirect_uri(config).to_string())
        .map_err(|_| AppError::InternalError)?;

    let client_id = ClientId::new(provider.client_id(config).to_string());
    let client_secret = ClientSecret::new(provider.client_secret(config).to_string());
    let client = BasicClient::new(client_id, Some(client_secret), auth_url, Some(token_url))
        .set_redirect_uri(redirect_uri);
    let mut request = client.authorize_url(CsrfToken::new_random);

    for scope in provider.scopes() {
        request = request.add_scope(Scope::new(scope.to_string()));
    }

    let (url, csrf) = request.url();
    Ok((url.to_string(), csrf.secret().to_string()))
}

fn build_oauth_state_cookie(name: &str, state: &str) -> Cookie<'static> {
    Cookie::build((name.to_string(), state.to_string()))
        .http_only(true)
        .secure(true)
        .same_site(SameSite::Lax)
        .path("/")
        .max_age(
            std::time::Duration::from_secs(600)
                .try_into()
                .unwrap_or_default(),
        )
        .build()
}

fn validate_oauth_state_cookie(
    jar: &CookieJar,
    provider: OAuthProvider,
    query_state: &str,
) -> Result<(), AppError> {
    let cookie_state = jar
        .get(provider.state_cookie_name())
        .map(|cookie| cookie.value().to_string())
        .ok_or(AppError::Unauthorized)?;

    if cookie_state != query_state {
        return Err(AppError::Unauthorized);
    }

    Ok(())
}

async fn exchange_code_and_fetch_identity<C: OAuthHttpClient>(
    http: &C,
    config: &Config,
    provider: OAuthProvider,
    code: &str,
) -> Result<OAuthIdentity, AppError> {
    let token_json = http
        .post_form(
            &provider.token_url(config),
            vec![
                (
                    "client_id".to_string(),
                    provider.client_id(config).to_string(),
                ),
                (
                    "client_secret".to_string(),
                    provider.client_secret(config).to_string(),
                ),
                ("grant_type".to_string(), "authorization_code".to_string()),
                ("code".to_string(), code.to_string()),
                (
                    "redirect_uri".to_string(),
                    provider.redirect_uri(config).to_string(),
                ),
            ],
        )
        .await?;

    let access_token = token_json
        .get("access_token")
        .and_then(|value| value.as_str())
        .ok_or(AppError::InternalError)?;

    let profile_json = http.get_json(provider.profile_url(), access_token).await?;

    let provider_id = profile_json
        .get("sub")
        .or_else(|| profile_json.get("oid"))
        .or_else(|| profile_json.get("id"))
        .and_then(|value| value.as_str())
        .ok_or(AppError::InternalError)?;

    let email = profile_json
        .get("email")
        .or_else(|| profile_json.get("mail"))
        .or_else(|| profile_json.get("userPrincipalName"))
        .and_then(|value| value.as_str())
        .ok_or_else(|| AppError::EmailRequired {
            message: "Email is required to continue OAuth sign in".to_string(),
            add_email_url: "/user/contacts".to_string(),
        })?;

    Ok(OAuthIdentity {
        provider_id: provider_id.to_string(),
        email: email.to_string(),
    })
}

async fn resolve_or_create_oauth_user(
    db: &PgPool,
    config: &Config,
    provider: OAuthProvider,
    provider_id: &str,
    email: &str,
) -> Result<OAuthUserResolution, AppError> {
    let user_id_by_oauth = sqlx::query_scalar::<_, Uuid>(
        "SELECT user_id FROM auth_oauth WHERE provider = $1 AND provider_id = $2",
    )
    .bind(provider.as_str())
    .bind(provider_id)
    .fetch_optional(db)
    .await
    .map_err(|_| AppError::InternalError)?;

    let idx_key = decode_hex_key_32(&config.encryption_index_key)?;
    let email_index = hmac_index(email, &idx_key);

    let conflicting_provider = sqlx::query_scalar::<_, String>(
        "SELECT ao.provider
         FROM user_contacts uc
         INNER JOIN auth_oauth ao ON ao.user_id = uc.user_id
         WHERE uc.email_index = $1
           AND ao.provider <> $2
         LIMIT 1",
    )
    .bind(&email_index)
    .bind(provider.as_str())
    .fetch_optional(db)
    .await
    .map_err(|_| AppError::InternalError)?;

    if let Some(existing_provider) = conflicting_provider {
        if let Some(details) = detect_email_conflict(&existing_provider, provider.as_str()) {
            tracing::warn!(
                existing_provider = %details.existing_provider,
                link_url = %details.link_url,
                "OAuth email conflict detected"
            );
            return Ok(OAuthUserResolution::Conflict(details));
        }
    }

    let user_id = match user_id_by_oauth {
        Some(id) => id,
        None => {
            let existing_same_provider_user = sqlx::query_scalar::<_, Uuid>(
                "SELECT ao.user_id
                 FROM user_contacts uc
                 INNER JOIN auth_oauth ao ON ao.user_id = uc.user_id
                 WHERE uc.email_index = $1
                   AND ao.provider = $2
                 LIMIT 1",
            )
            .bind(&email_index)
            .bind(provider.as_str())
            .fetch_optional(db)
            .await
            .map_err(|_| AppError::InternalError)?;

            if let Some(id) = existing_same_provider_user {
                id
            } else {
                let created_user_id = sqlx::query_scalar::<_, Uuid>(
                    "INSERT INTO user_accounts DEFAULT VALUES RETURNING id",
                )
                .fetch_one(db)
                .await
                .map_err(|_| AppError::InternalError)?;

                created_user_id
            }
        }
    };

    sqlx::query(
        "INSERT INTO auth_oauth (user_id, provider, provider_id)
         VALUES ($1, $2, $3)
         ON CONFLICT (provider, provider_id)
         DO UPDATE SET user_id = EXCLUDED.user_id",
    )
    .bind(user_id)
    .bind(provider.as_str())
    .bind(provider_id)
    .execute(db)
    .await
    .map_err(|_| AppError::InternalError)?;

    upsert_email_contact(db, config, user_id, email).await?;

    Ok(OAuthUserResolution::User(user_id))
}

async fn upsert_email_contact(
    db: &PgPool,
    config: &Config,
    user_id: Uuid,
    email: &str,
) -> Result<(), AppError> {
    let key = decode_hex_key_32(&config.encryption_key)?;
    let idx_key = decode_hex_key_32(&config.encryption_index_key)?;

    let (email_enc, nonce) = encrypt(email, &key)?;
    let email_index = hmac_index(email, &idx_key);

    sqlx::query(
        "INSERT INTO user_contacts (user_id, email_enc, email_nonce, email_index, updated_at)
         VALUES ($1, $2, $3, $4, extract(epoch from now()))
         ON CONFLICT (user_id)
         DO UPDATE SET
             email_enc = EXCLUDED.email_enc,
             email_nonce = EXCLUDED.email_nonce,
             email_index = EXCLUDED.email_index,
             updated_at = extract(epoch from now())",
    )
    .bind(user_id)
    .bind(email_enc)
    .bind(nonce.to_vec())
    .bind(email_index)
    .execute(db)
    .await
    .map_err(|_| AppError::InternalError)?;

    Ok(())
}

fn detect_email_conflict(
    existing_provider: &str,
    current_provider: &str,
) -> Option<EmailConflictDetails> {
    if existing_provider.eq_ignore_ascii_case(current_provider) {
        return None;
    }

    Some(EmailConflictDetails {
        existing_provider: existing_provider.to_string(),
        link_url: "/auth/link/oauth".to_string(),
    })
}

fn decode_hex_key_32(key_hex: &str) -> Result<[u8; 32], AppError> {
    let key_bytes = hex::decode(key_hex).map_err(|_| AppError::InternalError)?;
    key_bytes.try_into().map_err(|_| AppError::InternalError)
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
    let row = sqlx::query_scalar::<_, Uuid>(
        "DELETE FROM refresh_tokens
         WHERE token_hash = $1
           AND expires_at > extract(epoch from now())
         RETURNING user_id",
    )
    .bind(&token_hash)
    .fetch_optional(&db)
    .await
    .map_err(|_| AppError::InternalError)?;

    let user_id = row.ok_or(AppError::InvalidRefreshToken)?;

    // Fetch user's current wallet pubkey (may have changed)
    let pubkey =
        sqlx::query_scalar::<_, String>("SELECT pubkey FROM auth_wallet WHERE user_id = $1")
            .bind(user_id)
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

    sqlx::query("DELETE FROM refresh_tokens WHERE token_hash = $1")
        .bind(&token_hash)
        .execute(&db)
        .await
        .map_err(|_| AppError::InternalError)?;

    Ok(Json(
        serde_json::json!({ "message": "Logged out successfully" }),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use solana_sdk::signature::{Keypair, Signer};

    struct MockOAuthHttpClient {
        token_response: Value,
        profile_response: Value,
    }

    impl OAuthHttpClient for MockOAuthHttpClient {
        fn post_form<'a>(
            &'a self,
            _url: &'a str,
            _form: Vec<(String, String)>,
        ) -> Pin<Box<dyn Future<Output = Result<Value, AppError>> + Send + 'a>> {
            Box::pin(async move { Ok(self.token_response.clone()) })
        }

        fn get_json<'a>(
            &'a self,
            _url: &'a str,
            _bearer_token: &'a str,
        ) -> Pin<Box<dyn Future<Output = Result<Value, AppError>> + Send + 'a>> {
            Box::pin(async move { Ok(self.profile_response.clone()) })
        }
    }

    fn test_config() -> Config {
        Config {
            database_url: "postgres://localhost/test".to_string(),
            solana_rpc_url: "http://localhost:8899".to_string(),
            solana_ws_url: "ws://localhost:8900".to_string(),
            program_id: "program".to_string(),
            jwt_secret: "secret".to_string(),
            jwt_access_expiry_seconds: 900,
            jwt_refresh_expiry_seconds: 604800,
            encryption_key: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
                .to_string(),
            encryption_index_key:
                "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789".to_string(),
            google_client_id: "google-client-id".to_string(),
            google_client_secret: "google-client-secret".to_string(),
            google_redirect_uri: "https://api.pactum.app/auth/oauth/google/callback".to_string(),
            microsoft_client_id: "ms-client-id".to_string(),
            microsoft_client_secret: "ms-client-secret".to_string(),
            microsoft_redirect_uri: "https://api.pactum.app/auth/oauth/microsoft/callback"
                .to_string(),
            microsoft_tenant: "common".to_string(),
            resend_api_key: "resend".to_string(),
            email_from: "noreply@pactum.app".to_string(),
            invite_base_url: "https://app.pactum.app/invite".to_string(),
            invite_expiry_seconds: 604800,
            invite_reminder_after_seconds: 259200,
            platform_fee_usd_cents: 199,
            platform_fee_free_tier: 3,
            platform_nonrefundable_fee_cents: 10,
            platform_vault_pubkey: "vault".to_string(),
            platform_vault_keypair_path: std::path::PathBuf::from("/tmp/vault.json"),
            platform_treasury_pubkey: "treasury".to_string(),
            platform_treasury_keypair_path: std::path::PathBuf::from("/tmp/treasury.json"),
            vault_min_sol_alert: 0.5,
            vault_min_sol_circuit_breaker: 0.1,
            vault_funding_rate_limit_per_hour: 50,
            treasury_min_usdc_alert: 20_000_000,
            treasury_float_per_token: 50_000_000,
            treasury_sweep_dest: "sweep-dest".to_string(),
            stablecoin_registry: crate::config::StablecoinRegistry {
                usdc: crate::config::StablecoinInfo {
                    symbol: "usdc",
                    mint: "mint-usdc".to_string(),
                    ata: "ata-usdc".to_string(),
                    decimals: 6,
                },
                usdt: crate::config::StablecoinInfo {
                    symbol: "usdt",
                    mint: "mint-usdt".to_string(),
                    ata: "ata-usdt".to_string(),
                    decimals: 6,
                },
                pyusd: crate::config::StablecoinInfo {
                    symbol: "pyusd",
                    mint: "mint-pyusd".to_string(),
                    ata: "ata-pyusd".to_string(),
                    decimals: 6,
                },
            },
            pinata_jwt: "jwt".to_string(),
            pinata_gateway_domain: "gateway.pinata.cloud".to_string(),
            arweave_wallet_path: std::path::PathBuf::from("/tmp/arweave.json"),
            server_port: 8080,
            server_host: "0.0.0.0".to_string(),
        }
    }

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

    #[test]
    fn test_signature_verify_roundtrip() {
        let keypair = Keypair::new();
        let pubkey = keypair.pubkey();
        let nonce = Uuid::new_v4().to_string();

        let signature = keypair.sign_message(nonce.as_bytes());
        let parsed_signature = Signature::from_str(&signature.to_string()).unwrap();

        assert!(parsed_signature.verify(pubkey.as_ref(), nonce.as_bytes()));
    }

    #[test]
    fn test_google_redirect_url_contains_required_params() {
        let config = test_config();
        let (url, state) = build_provider_auth_url(&config, OAuthProvider::Google).unwrap();

        assert!(url.contains("client_id=google-client-id"));
        assert!(url.contains("redirect_uri="));
        assert!(url.contains("scope="));
        assert!(url.contains(&format!("state={}", state)));
    }

    #[test]
    fn test_email_conflict_detection_helper() {
        let conflict = detect_email_conflict("google", "microsoft").unwrap();
        assert_eq!(conflict.existing_provider, "google");
        assert_eq!(conflict.link_url, "/auth/link/oauth");

        assert!(detect_email_conflict("google", "google").is_none());
    }

    #[tokio::test]
    async fn test_callback_token_exchange_with_mock_http_client() {
        let config = test_config();
        let http = MockOAuthHttpClient {
            token_response: json!({ "access_token": "test_access" }),
            profile_response: json!({ "sub": "google-sub-123", "email": "user@example.com" }),
        };

        let identity =
            exchange_code_and_fetch_identity(&http, &config, OAuthProvider::Google, "test_code")
                .await
                .unwrap();

        assert_eq!(identity.provider_id, "google-sub-123");
        assert_eq!(identity.email, "user@example.com");
    }
    #[test]
    fn test_link_wallet_jwt_contains_pubkey() {
        // This test verifies that when a wallet is linked, the returned JWT includes the pubkey in claims
        // We'll parse the JWT and check the claims structure
        let config = test_config();
        let keypair = Keypair::new();
        let pubkey_str = keypair.pubkey().to_string();
        let user_id = uuid::Uuid::new_v4();

        // Issue an access token with pubkey
        let access_token = issue_access_token(user_id, Some(pubkey_str.clone()), &config)
            .expect("Failed to issue token");

        // Decode the token and verify pubkey is in claims
        let claims = decode_access_token(&access_token, &config).expect("Failed to decode token");

        // Verify the pubkey is in the decoded claims
        assert_eq!(claims.pubkey, Some(pubkey_str));
        assert_eq!(claims.sub, user_id);
    }
}
