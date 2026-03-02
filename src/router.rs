use axum::{
    extract::State,
    http::Method,
    routing::{delete, get, post, put},
    Router,
};
use http::header::{AUTHORIZATION, CONTENT_TYPE};
use tower_governor::{governor::GovernorConfigBuilder, GovernorLayer};
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

use crate::handlers;
use crate::middleware;
use crate::state::AppState;

/// Builds the complete router with middleware stack and all route groups.
/// Middleware order: CORS → Rate Limiting → Tracing
pub fn build_router(state: AppState) -> Router {
    // CORS configuration: whitelist frontend origins with specific methods/headers
    let cors = CorsLayer::new()
        .allow_origin([
            "https://pactum.app".parse().unwrap(),
            "https://app.pactum.app".parse().unwrap(),
        ])
        .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE])
        .allow_headers([AUTHORIZATION, CONTENT_TYPE]);

    // Rate limiting: 60 requests per second, burst of 100
    let governor = GovernorLayer::new(
        GovernorConfigBuilder::default()
            .per_second(60)
            .burst_size(100)
            .finish()
            .unwrap(),
    );

    // Build router with all route groups merged
    Router::new()
        .merge(auth_routes())
        .merge(upload_routes())
        .merge(agreement_routes())
        .merge(draft_routes())
        .merge(invite_routes())
        .merge(payment_routes())
        .merge(user_routes())
        .merge(ws_routes())
        .route("/health", get(|| async { "OK" }))
        // Apply middleware in correct order
        .layer(cors)
        .layer(governor)
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

// ---------------------------------------------------------------------------
// Auth routes
// ---------------------------------------------------------------------------
// Handlers challenge/verify/link_wallet/refresh/logout accept State<PgPool>
// and State<Config> individually, so we bridge via closures that destructure
// AppState. The OAuth routes already accept State<AppState>.
// ---------------------------------------------------------------------------

fn auth_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/auth/challenge",
            get(|State(state): State<AppState>| async move {
                handlers::auth::challenge(State(state.db.clone())).await
            }),
        )
        .route(
            "/auth/verify",
            post(
                |State(state): State<AppState>,
                 body: axum::Json<handlers::auth::VerifyRequest>| async move {
                    handlers::auth::verify(
                        State(state.db.clone()),
                        State(state.config.as_ref().clone()),
                        body,
                    )
                    .await
                },
            ),
        )
        .route(
            "/auth/oauth/google",
            get(handlers::auth::oauth_google),
        )
        .route(
            "/auth/oauth/google/callback",
            get(handlers::auth::oauth_google_callback),
        )
        .route(
            "/auth/oauth/microsoft",
            get(handlers::auth::oauth_microsoft),
        )
        .route(
            "/auth/oauth/microsoft/callback",
            get(handlers::auth::oauth_microsoft_callback),
        )
        .route(
            "/auth/link/wallet",
            post(
                |State(state): State<AppState>,
                 auth: middleware::auth::AuthUser,
                 body: axum::Json<handlers::auth::LinkWalletRequest>| async move {
                    handlers::auth::link_wallet(
                        State(state.db.clone()),
                        State(state.config.as_ref().clone()),
                        auth,
                        body,
                    )
                    .await
                },
            ),
        )
        .route(
            "/auth/refresh",
            post(
                |State(state): State<AppState>,
                 body: axum::Json<handlers::auth::RefreshRequest>| async move {
                    handlers::auth::refresh(
                        State(state.db.clone()),
                        State(state.config.as_ref().clone()),
                        body,
                    )
                    .await
                },
            ),
        )
        .route(
            "/auth/logout",
            post(
                |State(state): State<AppState>,
                 body: axum::Json<handlers::auth::RefreshRequest>| async move {
                    handlers::auth::logout(State(state.db.clone()), body).await
                },
            ),
        )
}

// ---------------------------------------------------------------------------
// Upload routes
// ---------------------------------------------------------------------------

fn upload_routes() -> Router<AppState> {
    Router::new().route(
        "/upload",
        post(
            |State(state): State<AppState>,
             auth: middleware::wallet_guard::AuthUserWithWallet,
             multipart: axum::extract::Multipart| async move {
                handlers::upload::upload_handler(
                    State(state.config.as_ref().clone()),
                    auth,
                    multipart,
                )
                .await
            },
        ),
    )
}

// ---------------------------------------------------------------------------
// Agreement routes — all handlers accept State<AppState> directly
// ---------------------------------------------------------------------------

fn agreement_routes() -> Router<AppState> {
    Router::new()
        .route("/agreement", post(handlers::agreement::create_agreement))
        .route("/agreement/:pda", get(handlers::agreement::get_agreement))
        .route("/agreements", get(handlers::agreement::list_agreements))
        .route(
            "/agreement/:pda/sign",
            post(handlers::agreement::sign_agreement),
        )
}

// ---------------------------------------------------------------------------
// Draft routes — all handlers accept State<AppState> directly
// ---------------------------------------------------------------------------

fn draft_routes() -> Router<AppState> {
    Router::new()
        .route("/draft/:id", get(handlers::draft::get_draft))
        .route("/draft/:id", delete(handlers::draft::delete_draft))
        .route("/draft/:id/reinvite", put(handlers::draft::reinvite_draft))
        .route("/draft/:id/submit", post(handlers::draft::submit_draft))
}

// ---------------------------------------------------------------------------
// Invite routes — all handlers accept State<AppState> directly
// ---------------------------------------------------------------------------

fn invite_routes() -> Router<AppState> {
    Router::new()
        .route("/invite/:token", get(handlers::invite::get_invite))
        .route(
            "/invite/:token/accept",
            post(handlers::invite::accept_invite),
        )
}

// ---------------------------------------------------------------------------
// Payment routes — all handlers accept State<AppState> directly
// ---------------------------------------------------------------------------

fn payment_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/payment/initiate/:draft_id",
            post(handlers::payment::initiate_payment),
        )
        .route(
            "/payment/status/:draft_id",
            get(handlers::payment::payment_status),
        )
}

// ---------------------------------------------------------------------------
// User routes
// ---------------------------------------------------------------------------
// Handlers accept State<PgPool> and State<Config>, bridged via closures.
// ---------------------------------------------------------------------------

fn user_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/user/me",
            get(
                |State(state): State<AppState>, auth: middleware::auth::AuthUser| async move {
                    handlers::user::get_profile(State(state.db.clone()), auth).await
                },
            )
            .put(
                |State(state): State<AppState>,
                 auth: middleware::auth::AuthUser,
                 body: axum::Json<handlers::user::UpdateProfileRequest>| async move {
                    handlers::user::update_profile(State(state.db.clone()), auth, body).await
                },
            ),
        )
        .route(
            "/user/contacts",
            put(
                |State(state): State<AppState>,
                 auth: middleware::auth::AuthUser,
                 body: axum::Json<handlers::user::UpdateContactsRequest>| async move {
                    handlers::user::update_contacts(
                        State(state.db.clone()),
                        State(state.config.as_ref().clone()),
                        auth,
                        body,
                    )
                    .await
                },
            )
            .delete(
                |State(state): State<AppState>, auth: middleware::auth::AuthUser| async move {
                    handlers::user::delete_contacts(State(state.db.clone()), auth).await
                },
            ),
        )
}

// ---------------------------------------------------------------------------
// WebSocket routes — handler accepts State<AppState> directly
// ---------------------------------------------------------------------------

fn ws_routes() -> Router<AppState> {
    Router::new().route("/ws", get(handlers::ws::ws_handler))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_router_builds() {
        // Smoke test: verify code compiles and imports are correct
        // Actual router instantiation in integration tests with AppState
    }
}
