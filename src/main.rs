pub mod config;
pub mod error;
pub mod router;
pub mod solana_types;
pub mod state;

pub mod handlers {
    pub mod agreement;
    pub mod auth;
    pub mod draft;
    pub mod invite;
    pub mod payment;
    pub mod upload;
    pub mod user;
    pub mod ws;
}

pub mod services {
    pub mod crypto;
    pub mod hash;
    pub mod jwt;
    pub mod keypair_security;
    pub mod metadata;
    pub mod notification;
    pub mod refund;
    pub mod solana;
    pub mod solana_pay;
    pub mod storage;
}

pub mod middleware {
    pub mod auth;
    pub mod wallet_guard;
}

pub mod workers {
    pub mod event_listener;
    pub mod keeper;
    pub mod notification_worker;
    pub mod refund_worker;
}

use axum::{routing::get, routing::post, Router};
use config::Config;
use dashmap::DashMap;
use solana_client::rpc_client::RpcClient;
use sqlx::postgres::PgPoolOptions;
use state::{AppState, ProtectedKeypair};
use std::sync::Arc;
use tokio::sync::broadcast;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load .env file
    dotenvy::dotenv().ok();

    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    tracing::info!("🚀 Pactum backend starting...");

    // Load configuration
    let config = Config::from_env()?;
    tracing::info!("✓ Configuration loaded");

    // Connect to PostgreSQL
    let db = PgPoolOptions::new()
        .max_connections(20)
        .connect(&config.database_url)
        .await
        .expect("Failed to connect to PostgreSQL");
    tracing::info!("✓ PostgreSQL connected");

    // Run migrations
    sqlx::migrate!("./migrations")
        .run(&db)
        .await
        .expect("Failed to run migrations");
    tracing::info!("✓ Migrations applied");

    // Load keypairs
    let vault_keypair = services::keypair_security::load_keypair(
        config.platform_vault_keypair_path.to_str().expect("Invalid vault keypair path")
    ).expect("Failed to load vault keypair");
    tracing::info!("✓ Vault keypair loaded");

    let treasury_keypair = services::keypair_security::load_keypair(
        config.platform_treasury_keypair_path.to_str().expect("Invalid treasury keypair path")
    ).expect("Failed to load treasury keypair");
    tracing::info!("✓ Treasury keypair loaded");

    // Create Solana RPC client
    let rpc_client = Arc::new(RpcClient::new(config.solana_rpc_url.clone()));
    tracing::info!("✓ Solana RPC client initialized");

    // Create WebSocket broadcast channels
    let (ws_tx, _) = broadcast::channel(1000);
    let ws_connections = Arc::new(DashMap::new());

    // Build AppState
    let state = AppState {
        db: db.clone(),
        config: config.clone(),
        rpc_client: rpc_client.clone(),
        vault_keypair,
        treasury_keypair,
        ws_tx,
        ws_connections,
    };

    // Validate keypair pubkeys
    services::keypair_security::validate_keypair_pubkeys(&state);
    tracing::info!("✓ Keypair pubkeys validated");

    // Build router with auth routes
    let app = Router::new()
        // Auth routes
        .route("/auth/challenge", get(handlers::auth::challenge))
        .route("/auth/verify", post(handlers::auth::verify))
        .route("/auth/refresh", post(handlers::auth::refresh))
        .route("/auth/logout", post(handlers::auth::logout))
        // Health check
        .route("/health", get(|| async { "OK" }))
        // Attach state
        .with_state(db)
        .with_state(config.clone());

    // Start server
    let addr = format!("{}:{}", config.server_host, config.server_port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    
    tracing::info!("🎉 Pactum backend listening on {}", addr);
    
    axum::serve(listener, app).await?;

    Ok(())
}
