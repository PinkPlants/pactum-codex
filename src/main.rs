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
    pub mod program_log;
    pub mod refund;
    pub mod solana;
    pub mod solana_logs;
    pub mod solana_pay;
    pub mod storage;
}

pub mod middleware {
    pub mod auth;
    pub mod wallet_guard;
}

pub mod workers {
    pub mod event_listener;
    pub mod expiry_worker;
    pub mod keeper;
    pub mod notification_worker;
    pub mod refund_worker;
}

use config::Config;
use dashmap::DashMap;
use solana_client::rpc_client::RpcClient;
use sqlx::postgres::PgPoolOptions;
use state::{AppState, WsEvent};
use std::sync::Arc;
use tokio::sync::broadcast;
use uuid::Uuid;

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
    let config = Arc::new(Config::from_env());
    tracing::info!("✓ Configuration loaded");

    // Connect to PostgreSQL
    let db = PgPoolOptions::new()
        .max_connections(20)
        .connect(&config.database_url)
        .await
        .expect("Failed to connect to PostgreSQL");
    tracing::info!("✓ PostgreSQL connected");

    // Run migrations
    sqlx::migrate!("./migrations").run(&db).await?;
    tracing::info!("✓ Migrations applied");

    // Load keypairs
    let vault_keypair_path = config.platform_vault_keypair_path.to_str().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "invalid vault keypair path",
        )
    })?;
    let vault_keypair = Arc::new(
        services::keypair_security::load_keypair(vault_keypair_path)
            .expect("Failed to load vault keypair"),
    );
    tracing::info!("✓ Vault keypair loaded");

    let treasury_keypair_path =
        config
            .platform_treasury_keypair_path
            .to_str()
            .ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "invalid treasury keypair path",
                )
            })?;
    let treasury_keypair = Arc::new(
        services::keypair_security::load_keypair(treasury_keypair_path)
            .expect("Failed to load treasury keypair"),
    );
    tracing::info!("✓ Treasury keypair loaded");

    // Create Solana RPC client
    let solana = Arc::new(RpcClient::new(config.solana_rpc_url.clone()));
    tracing::info!("✓ Solana RPC client initialized");

    let server_addr = format!("{}:{}", config.server_host, config.server_port);
    let ws_channels: Arc<DashMap<Uuid, broadcast::Sender<WsEvent>>> = Arc::new(DashMap::new());

    // Build AppState
    let state = AppState {
        db,
        config: Arc::clone(&config),
        solana,
        vault_keypair,
        treasury_keypair,
        ws_channels,
    };

    // Validate keypair pubkeys
    services::keypair_security::validate_keypair_pubkeys(&state);
    tracing::info!("✓ Keypair pubkeys validated");

    // Spawn background workers
    tokio::spawn(workers::event_listener::run(state.clone()));
    tokio::spawn(workers::keeper::run(state.clone()));
    tokio::spawn(workers::notification_worker::run(state.clone()));
    tokio::spawn(workers::refund_worker::run(state.clone()));
    tokio::spawn(workers::expiry_worker::run(state.clone()));
    tracing::info!("✓ Background workers spawned");

    // Build router (all routes wired inside router::build_router)
    let app = router::build_router(state);

    // Start server
    let listener = tokio::net::TcpListener::bind(&server_addr).await?;
    tracing::info!("🎉 Pactum backend listening on {}", server_addr);
    axum::serve(listener, app).await?;

    Ok(())
}
