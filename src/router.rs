use axum::{
    extract::State,
    http::Method,
    http::StatusCode,
    routing::{delete, get, post, put},
    Json, Router,
};
use http::header::{AUTHORIZATION, CONTENT_TYPE};
use serde::Serialize;
use tower_governor::{governor::GovernorConfigBuilder, GovernorLayer};
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

use crate::handlers;
use crate::middleware;
use crate::state::{AppState, ProcessHealth};

/// Builds the complete router with middleware stack and all route groups.
/// Middleware order: CORS → Rate Limiting → Tracing
pub fn build_router(state: AppState) -> Router {
    // CORS configuration: whitelist frontend origins with specific methods/headers
    // FAIL-FAST: Static invariants that panic at startup if misconfigured.
    // These unwrap() calls are compile-time constants (valid URLs). If they
    // fail, the binary is miscompiled and must not start.
    // See: startup_fatal_path.md#static-invariants
    let cors = CorsLayer::new()
        .allow_origin([
            "https://pactum.app".parse().unwrap(),
            "https://app.pactum.app".parse().unwrap(),
        ])
        .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE])
        .allow_headers([AUTHORIZATION, CONTENT_TYPE]);

    // Rate limiting: 60 requests per second, burst of 100
    // FAIL-FAST: Governor config with valid parameters (per_second > 0, burst_size > 0)
    // cannot fail. If it does, it's a programming error and the service must not start.
    // See: startup_fatal_path.md#static-invariants
    let governor = GovernorLayer {
        config: std::sync::Arc::new(
            GovernorConfigBuilder::default()
                .per_second(60)
                .burst_size(100)
                .finish()
                .expect("Governor config with valid parameters cannot fail"),
        ),
    };

    let api_routes = Router::new()
        .merge(auth_routes())
        .merge(upload_routes())
        .merge(agreement_routes())
        .merge(draft_routes())
        .merge(invite_routes())
        .merge(payment_routes())
        .merge(user_routes())
        .merge(ws_routes())
        .layer(governor);

    Router::new()
        .route("/health", get(health_handler))
        .merge(api_routes)
        // Apply middleware in correct order
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

#[derive(Debug, Serialize, PartialEq, Eq)]
struct HealthResponse {
    status: &'static str,
    process_alive: bool,
    workers: &'static str,
}

async fn health_handler(State(state): State<AppState>) -> (StatusCode, Json<HealthResponse>) {
    health_response(state.process_health.current())
}

fn health_response(status: ProcessHealth) -> (StatusCode, Json<HealthResponse>) {
    match status {
        ProcessHealth::Healthy => (
            StatusCode::OK,
            Json(HealthResponse {
                status: "healthy",
                process_alive: true,
                workers: "ready",
            }),
        ),
        ProcessHealth::Degraded => (
            StatusCode::OK,
            Json(HealthResponse {
                status: "degraded",
                process_alive: true,
                workers: "degraded",
            }),
        ),
        ProcessHealth::StartupFailed => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(HealthResponse {
                status: "startup-failed",
                process_alive: false,
                workers: "unavailable",
            }),
        ),
        ProcessHealth::RuntimeFailed => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(HealthResponse {
                status: "runtime-failed",
                process_alive: true,
                workers: "failed",
            }),
        ),
    }
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
        .route(
            "/agreement/:pda/cancel",
            post(handlers::agreement::cancel_agreement),
        )
        .route(
            "/agreement/:pda/revoke",
            post(handlers::agreement::vote_revoke),
        )
        .route(
            "/agreement/:pda/retract",
            post(handlers::agreement::retract_revoke_vote),
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
    use crate::config::{Config, StablecoinInfo, StablecoinRegistry};
    use crate::state::{ProcessHealthState, ProtectedKeypair, WsEvent};
    use crate::workers::{
        policy::WorkerCriticality,
        supervisor::{WorkerLifecycle, WorkerSupervisor},
    };
    use axum::{
        body::{to_bytes, Body},
        http::Request,
    };
    use dashmap::DashMap;
    use solana_client::rpc_client::RpcClient;
    use solana_sdk::signature::Keypair;
    use std::{path::PathBuf, sync::Arc, time::Duration};
    use tokio::sync::broadcast;
    use tower::util::ServiceExt;
    use uuid::Uuid;

    fn test_config() -> Config {
        Config {
            database_url: "postgres://pactum@localhost:5432/pactum".to_string(),
            solana_rpc_url: "http://localhost:8899".to_string(),
            solana_ws_url: "ws://localhost:8900".to_string(),
            program_id: "DF1cHTN9EE8Qonda1esTeYvFjmbYcoc52vDTjTMKvS1P".to_string(),
            jwt_secret: "test_jwt_secret".to_string(),
            jwt_access_expiry_seconds: 900,
            jwt_refresh_expiry_seconds: 604800,
            encryption_key: "test_encryption_key".to_string(),
            encryption_index_key: "test_encryption_index_key".to_string(),
            google_client_id: "test_google_client_id".to_string(),
            google_client_secret: "test_google_client_secret".to_string(),
            google_redirect_uri: "https://api.pactum.app/auth/oauth/google/callback".to_string(),
            microsoft_client_id: "test_microsoft_client_id".to_string(),
            microsoft_client_secret: "test_microsoft_client_secret".to_string(),
            microsoft_redirect_uri: "https://api.pactum.app/auth/oauth/microsoft/callback"
                .to_string(),
            microsoft_tenant: "common".to_string(),
            resend_api_key: "test_resend_key".to_string(),
            email_from: "noreply@pactum.app".to_string(),
            invite_base_url: "https://app.pactum.app/invite".to_string(),
            invite_expiry_seconds: 604800,
            invite_reminder_after_seconds: 259200,
            platform_fee_usd_cents: 199,
            platform_fee_free_tier: 3,
            platform_nonrefundable_fee_cents: 10,
            platform_vault_pubkey: "test_vault_pubkey".to_string(),
            platform_vault_keypair_path: PathBuf::from("/tmp/vault.json"),
            platform_treasury_pubkey: "test_treasury_pubkey".to_string(),
            platform_treasury_keypair_path: PathBuf::from("/tmp/treasury.json"),
            vault_min_sol_alert: 0.5,
            vault_min_sol_circuit_breaker: 0.1,
            vault_funding_rate_limit_per_hour: 50,
            treasury_min_usdc_alert: 20_000_000,
            treasury_float_per_token: 50_000_000,
            treasury_sweep_dest: "test_sweep_dest".to_string(),
            stablecoin_registry: StablecoinRegistry {
                usdc: StablecoinInfo {
                    symbol: "usdc",
                    mint: "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v".to_string(),
                    ata: "test_usdc_ata".to_string(),
                    decimals: 6,
                },
                usdt: StablecoinInfo {
                    symbol: "usdt",
                    mint: "Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB".to_string(),
                    ata: "test_usdt_ata".to_string(),
                    decimals: 6,
                },
                pyusd: StablecoinInfo {
                    symbol: "pyusd",
                    mint: "2b1kV6DkPAnxd5ixfnxCpjxmKwqjjaYmCZfHsFu24GXo".to_string(),
                    ata: "test_pyusd_ata".to_string(),
                    decimals: 6,
                },
            },
            pinata_jwt: "test_pinata_jwt".to_string(),
            pinata_gateway_domain: "gateway.pinata.cloud".to_string(),
            arweave_wallet_path: PathBuf::from("/tmp/arweave-wallet.json"),
            server_port: 8080,
            server_host: "127.0.0.1".to_string(),
        }
    }

    fn test_app_state(health: ProcessHealth) -> AppState {
        AppState {
            db: sqlx::postgres::PgPoolOptions::new()
                .connect_lazy("postgres://pactum@localhost:5432/pactum")
                .expect("test database URL should be valid"),
            config: Arc::new(test_config()),
            solana: Arc::new(RpcClient::new("http://localhost:8899".to_string())),
            vault_keypair: Arc::new(ProtectedKeypair(Keypair::new())),
            treasury_keypair: Arc::new(ProtectedKeypair(Keypair::new())),
            ws_channels: Arc::new(DashMap::<Uuid, broadcast::Sender<WsEvent>>::new()),
            process_health: Arc::new(ProcessHealthState::new(health)),
        }
    }

    async fn fetch_health(app: Router) -> (StatusCode, serde_json::Value) {
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .method("GET")
                    .body(Body::empty())
                    .expect("health request should build"),
            )
            .await
            .expect("health route should respond");

        let status = response.status();
        let bytes = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("health body should be readable");
        let body: serde_json::Value =
            serde_json::from_slice(&bytes).expect("health response should be valid json");

        (status, body)
    }

    fn health_test_router(state: AppState) -> Router {
        Router::new()
            .route("/health", get(health_handler))
            .with_state(state)
    }

    #[test]
    fn health_contract_healthy() {
        let (status, body) = health_response(ProcessHealth::Healthy);
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            body.0,
            HealthResponse {
                status: "healthy",
                process_alive: true,
                workers: "ready",
            }
        );
    }

    #[test]
    fn health_contract_degraded() {
        let (status, body) = health_response(ProcessHealth::Degraded);
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            body.0,
            HealthResponse {
                status: "degraded",
                process_alive: true,
                workers: "degraded",
            }
        );
    }

    #[test]
    fn health_contract_startup_failed() {
        let (status, body) = health_response(ProcessHealth::StartupFailed);
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(
            body.0,
            HealthResponse {
                status: "startup-failed",
                process_alive: false,
                workers: "unavailable",
            }
        );
    }

    #[test]
    fn health_contract_runtime_failed() {
        let (status, body) = health_response(ProcessHealth::RuntimeFailed);
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(
            body.0,
            HealthResponse {
                status: "runtime-failed",
                process_alive: true,
                workers: "failed",
            }
        );
    }

    #[tokio::test]
    async fn health_route_reports_healthy_when_workers_are_ok() {
        let app = health_test_router(test_app_state(ProcessHealth::Healthy));
        let (status, body) = fetch_health(app).await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["status"], "healthy");
        assert_eq!(body["process_alive"], true);
        assert_eq!(body["workers"], "ready");
    }

    #[tokio::test]
    async fn health_route_reports_degraded_after_runtime_worker_failure() {
        let state = test_app_state(ProcessHealth::Healthy);
        let supervisor = WorkerSupervisor::new();

        let panicking_worker = supervisor.spawn(
            "runtime_panicking_worker",
            WorkerCriticality::NonCritical,
            Arc::clone(&state.process_health),
            || async {
                panic!("synthetic runtime worker failure");
            },
        );

        panicking_worker
            .await
            .expect("supervisor task should complete for panic case");
        tokio::time::sleep(Duration::from_millis(20)).await;

        assert_eq!(
            supervisor.lifecycle_of("runtime_panicking_worker"),
            Some(WorkerLifecycle::Panicked)
        );

        let app = health_test_router(state);
        let (status, body) = fetch_health(app).await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["status"], "degraded");
        assert_eq!(body["process_alive"], true);
        assert_eq!(body["workers"], "degraded");
    }

    #[tokio::test]
    async fn health_route_keeps_startup_failed_separate_from_degraded_runtime_state() {
        let app = health_test_router(test_app_state(ProcessHealth::StartupFailed));
        let (status, body) = fetch_health(app).await;

        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(body["status"], "startup-failed");
        assert_eq!(body["process_alive"], false);
        assert_eq!(body["workers"], "unavailable");
    }

    #[tokio::test]
    async fn health_route_reports_runtime_failed_for_critical_runtime_worker_failure() {
        let state = test_app_state(ProcessHealth::Healthy);
        let supervisor = WorkerSupervisor::new();

        let critical_worker = supervisor.spawn(
            "runtime_critical_worker",
            WorkerCriticality::Critical,
            Arc::clone(&state.process_health),
            || async {
                panic!("synthetic critical runtime worker failure");
            },
        );

        critical_worker
            .await
            .expect("supervisor task should complete for critical panic case");
        tokio::time::sleep(Duration::from_millis(20)).await;

        assert_eq!(
            supervisor.lifecycle_of("runtime_critical_worker"),
            Some(WorkerLifecycle::Panicked)
        );

        let app = health_test_router(state);
        let (status, body) = fetch_health(app).await;

        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(body["status"], "runtime-failed");
        assert_eq!(body["process_alive"], true);
        assert_eq!(body["workers"], "failed");
    }

    #[test]
    fn test_router_builds() {
        // Smoke test: verify code compiles and imports are correct
        // Actual router instantiation in integration tests with AppState
    }

    // =========================================================================
    // STARTUP FATAL PATH TESTS
    // =========================================================================
    // These tests document and guard the intentional fail-fast behaviors.
    // See: startup_fatal_path.md#static-invariants

    /// Test that CORS origin URLs are valid at compile time.
    /// FAIL-FAST CATEGORY: Static Invariant
    /// If this fails, the CORS configuration contains invalid URLs.
    #[test]
    fn test_cors_origins_are_valid_urls() {
        // These are the same URLs used in build_router(). If they fail to
        // parse, the service must not start (the unwrap() in build_router
        // will panic). This test documents that requirement.
        let origins = ["https://pactum.app", "https://app.pactum.app"];

        for origin in origins {
            let parsed: axum::http::HeaderValue = origin
                .parse()
                .expect("CORS origin must be a valid URL - this is a static invariant");
            assert!(
                parsed.to_str().unwrap().starts_with("https://"),
                "CORS origins must use HTTPS"
            );
        }
    }

    /// Test that rate limiting config is valid.
    /// FAIL-FAST CATEGORY: Static Invariant
    /// Governor config with per_second > 0 and burst_size > 0 cannot fail.
    #[test]
    fn test_rate_limiting_config_is_valid() {
        use tower_governor::governor::GovernorConfigBuilder;

        // This is the same config used in build_router()
        // If this fails, the service must not start.
        let _config = GovernorConfigBuilder::default()
            .per_second(60)
            .burst_size(100)
            .finish()
            .expect("Governor config with valid parameters cannot fail");
    }
}
