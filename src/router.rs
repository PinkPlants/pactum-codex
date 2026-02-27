use axum::{
    http::Method,
    routing::{delete, get, post, put},
    Router,
};
use http::header::{AUTHORIZATION, CONTENT_TYPE};
use tower_governor::{governor::GovernorConfigBuilder, GovernorLayer};
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

use crate::state::AppState;

/// Builds the complete router with middleware stack and all route groups
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

    // Rate limiting: 100 req/min general (per-IP via SmartIpKeyExtractor)
    // 60 requests per second, burst of 100
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
        // Apply middleware in correct order
        .layer(cors)
        .layer(governor)
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

/// Authentication routes (SIWS challenge/verify)
fn auth_routes() -> Router<AppState> {
    Router::new()
    // Routes will be added in Wave 1 Task 7+
}

/// File/image upload routes
fn upload_routes() -> Router<AppState> {
    Router::new()
    // Routes will be added in Wave 1 Task 8+
}

/// Agreement CRUD and state management routes
fn agreement_routes() -> Router<AppState> {
    Router::new()
    // Routes will be added in Wave 1 Task 9+
}

/// Draft agreement routes (before signing)
fn draft_routes() -> Router<AppState> {
    Router::new()
    // Routes will be added in Wave 1 Task 10+
}

/// Invite/participant management routes
fn invite_routes() -> Router<AppState> {
    Router::new()
    // Routes will be added in Wave 1 Task 11+
}

/// Payment and transaction routes
fn payment_routes() -> Router<AppState> {
    Router::new()
    // Routes will be added in Wave 1 Task 12+
}

/// User profile and account routes
fn user_routes() -> Router<AppState> {
    Router::new()
    // Routes will be added in Wave 1 Task 13+
}

/// WebSocket routes (real-time notifications)
fn ws_routes() -> Router<AppState> {
    Router::new()
    // Routes will be added in Wave 1 Task 14+
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
