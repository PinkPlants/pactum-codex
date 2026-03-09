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
    pub mod guardrails;
    pub mod keeper;
    pub mod notification_worker;
    pub mod policy;
    pub mod refund_worker;
    pub mod supervisor;
}

use config::Config;
use dashmap::DashMap;
use solana_client::rpc_client::RpcClient;
use sqlx::postgres::PgPoolOptions;
use state::{AppState, ProcessHealth, ProcessHealthState, ProtectedKeypair, WsEvent};
use std::path::Path;
use std::sync::Arc;
use tokio::sync::broadcast;
use uuid::Uuid;
use workers::policy::{BootstrapFailureAction, WorkerCriticality};

const REQUIRED_BOOTSTRAP_CRITICALITY: WorkerCriticality = WorkerCriticality::Critical;

#[derive(Debug, thiserror::Error)]
enum StartupError {
    #[error("required startup configuration missing or invalid: {0}")]
    MissingRequiredConfig(String),

    #[error("database unavailable during bootstrap: {0}")]
    DatabaseUnavailable(#[source] sqlx::Error),

    #[error("database migration failed during bootstrap: {0}")]
    MigrationFailed(#[source] sqlx::migrate::MigrateError),

    #[error("invalid {role} keypair path (non UTF-8)")]
    InvalidKeypairPath { role: &'static str },

    #[error("required {role} keypair could not be loaded: {source}")]
    KeypairUnavailable {
        role: &'static str,
        #[source]
        source: crate::error::AppError,
    },
}

#[must_use]
fn required_bootstrap_failure_action() -> BootstrapFailureAction {
    REQUIRED_BOOTSTRAP_CRITICALITY.on_bootstrap_failure()
}

fn log_bootstrap_failure(error: &StartupError) {
    match required_bootstrap_failure_action() {
        BootstrapFailureAction::FailFast => {
            tracing::error!(
                action = "fail_fast",
                bootstrap_boundary = "fatal",
                error = %error,
                "Fatal bootstrap dependency failed; aborting startup"
            );
        }
        BootstrapFailureAction::DisableAndContinue => {
            tracing::warn!(
                action = "disable_and_continue",
                bootstrap_boundary = "degradable",
                error = %error,
                "Bootstrap dependency failed but policy allowed continuing"
            );
        }
    }
}

fn panic_payload_to_string(payload: Box<dyn std::any::Any + Send>) -> String {
    if let Some(msg) = payload.downcast_ref::<String>() {
        return msg.clone();
    }
    if let Some(msg) = payload.downcast_ref::<&str>() {
        return (*msg).to_string();
    }
    "unknown panic payload".to_string()
}

fn load_required_config_with<F>(loader: F) -> Result<Arc<Config>, StartupError>
where
    F: FnOnce() -> Config + std::panic::UnwindSafe,
{
    std::panic::catch_unwind(loader)
        .map(Arc::new)
        .map_err(|panic| StartupError::MissingRequiredConfig(panic_payload_to_string(panic)))
}

fn load_required_config() -> Result<Arc<Config>, StartupError> {
    load_required_config_with(Config::from_env)
}

async fn connect_required_database_with<Connect, Fut>(
    database_url: &str,
    connect: Connect,
) -> Result<sqlx::PgPool, StartupError>
where
    Connect: FnOnce(String) -> Fut,
    Fut: std::future::Future<Output = Result<sqlx::PgPool, sqlx::Error>>,
{
    connect(database_url.to_owned())
        .await
        .map_err(StartupError::DatabaseUnavailable)
}

async fn connect_required_database(database_url: &str) -> Result<sqlx::PgPool, StartupError> {
    connect_required_database_with(database_url, |url| async move {
        PgPoolOptions::new().max_connections(20).connect(&url).await
    })
    .await
}

async fn run_required_migrations(db: &sqlx::PgPool) -> Result<(), StartupError> {
    sqlx::migrate!("./migrations")
        .run(db)
        .await
        .map_err(StartupError::MigrationFailed)
}

fn load_required_keypair(
    path: &Path,
    role: &'static str,
) -> Result<Arc<ProtectedKeypair>, StartupError> {
    let keypair_path = path
        .to_str()
        .ok_or(StartupError::InvalidKeypairPath { role })?;

    services::keypair_security::load_keypair(keypair_path)
        .map(Arc::new)
        .map_err(|source| StartupError::KeypairUnavailable { role, source })
}

async fn bootstrap_state() -> Result<AppState, StartupError> {
    let config = load_required_config()?;
    tracing::info!("✓ Configuration loaded");

    // FAIL-FAST: Database is core infrastructure. Without it, the service cannot
    // function. This is an intentional startup abort, not a runtime error.
    // See: startup_fatal_path.md#core-infrastructure
    let db = connect_required_database(&config.database_url).await?;
    tracing::info!("✓ PostgreSQL connected");

    run_required_migrations(&db).await?;
    tracing::info!("✓ Migrations applied");

    // FAIL-FAST: Platform keypairs are required for signing transactions.
    // If the keypair files are missing, unreadable, or invalid, the service
    // cannot perform its core function and must abort immediately.
    // See: startup_fatal_path.md#required-credentials
    let vault_keypair = load_required_keypair(&config.platform_vault_keypair_path, "vault")?;
    tracing::info!("✓ Vault keypair loaded");

    // FAIL-FAST: Treasury keypair is required for refund operations.
    // See: startup_fatal_path.md#required-credentials
    let treasury_keypair =
        load_required_keypair(&config.platform_treasury_keypair_path, "treasury")?;
    tracing::info!("✓ Treasury keypair loaded");

    // Create Solana RPC client
    let solana = Arc::new(RpcClient::new(config.solana_rpc_url.clone()));
    tracing::info!("✓ Solana RPC client initialized");

    let ws_channels: Arc<DashMap<Uuid, broadcast::Sender<WsEvent>>> = Arc::new(DashMap::new());
    let process_health = Arc::new(ProcessHealthState::new(ProcessHealth::StartupFailed));

    // Build AppState
    let state = AppState {
        db,
        config,
        solana,
        vault_keypair,
        treasury_keypair,
        ws_channels,
        process_health,
    };

    // Validate keypair pubkeys
    services::keypair_security::validate_keypair_pubkeys(&state);
    tracing::info!("✓ Keypair pubkeys validated");

    Ok(state)
}

fn spawn_runtime_workers(state: &AppState) {
    let worker_supervisor = workers::supervisor::WorkerSupervisor::new();

    worker_supervisor.spawn(
        "event_listener",
        WorkerCriticality::NonCritical,
        state.process_health.clone(),
        {
            let worker_state = state.clone();
            move || workers::event_listener::run(worker_state)
        },
    );
    worker_supervisor.spawn(
        "keeper",
        WorkerCriticality::Critical,
        state.process_health.clone(),
        {
            let worker_state = state.clone();
            move || workers::keeper::run(worker_state)
        },
    );
    worker_supervisor.spawn(
        "notification_worker",
        WorkerCriticality::NonCritical,
        state.process_health.clone(),
        {
            let worker_state = state.clone();
            move || workers::notification_worker::run(worker_state)
        },
    );
    worker_supervisor.spawn(
        "refund_worker",
        WorkerCriticality::NonCritical,
        state.process_health.clone(),
        {
            let worker_state = state.clone();
            move || workers::refund_worker::run(worker_state)
        },
    );
    worker_supervisor.spawn(
        "expiry_worker",
        WorkerCriticality::NonCritical,
        state.process_health.clone(),
        {
            let worker_state = state.clone();
            move || workers::expiry_worker::run(worker_state)
        },
    );
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load .env file
    dotenvy::dotenv().ok();

    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    tracing::info!("🚀 Pactum backend starting...");

    let state = match bootstrap_state().await {
        Ok(state) => state,
        Err(error) => {
            log_bootstrap_failure(&error);
            return Err(Box::new(error) as Box<dyn std::error::Error>);
        }
    };

    let server_addr = format!("{}:{}", state.config.server_host, state.config.server_port);

    state.process_health.set(ProcessHealth::Healthy);

    spawn_runtime_workers(&state);
    tracing::info!("✓ Background workers spawned");

    // Build router (all routes wired inside router::build_router)
    let app = router::build_router(state);

    // Start server
    let listener = tokio::net::TcpListener::bind(&server_addr).await?;
    tracing::info!("🎉 Pactum backend listening on {}", server_addr);
    axum::serve(listener, app).await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn required_bootstrap_dependencies_stay_fail_fast() {
        assert_eq!(
            required_bootstrap_failure_action(),
            BootstrapFailureAction::FailFast
        );
    }

    #[test]
    fn missing_env_config_failure_remains_bootstrap_fatal() {
        let result = load_required_config_with(|| panic!("DATABASE_URL is required"));
        let error = match result {
            Ok(_) => panic!("missing env config should abort bootstrap"),
            Err(error) => error,
        };

        assert!(matches!(
            error,
            StartupError::MissingRequiredConfig(message) if message.contains("DATABASE_URL is required")
        ));
        assert_eq!(
            required_bootstrap_failure_action(),
            BootstrapFailureAction::FailFast
        );
    }

    #[tokio::test]
    async fn database_connection_failure_remains_bootstrap_fatal() {
        let result =
            connect_required_database_with("postgres://pactum@localhost/pactum", |_| async {
                Err(sqlx::Error::Configuration(Box::new(std::io::Error::other(
                    "simulated db outage",
                ))))
            })
            .await;

        let error = result.expect_err("database connection failure should abort bootstrap");

        assert!(matches!(error, StartupError::DatabaseUnavailable(_)));
        assert_eq!(
            required_bootstrap_failure_action(),
            BootstrapFailureAction::FailFast
        );
    }

    #[test]
    fn unreadable_keypair_failure_remains_bootstrap_fatal() {
        let missing_path = Path::new("/definitely/missing/vault-keypair.json");
        let result = load_required_keypair(missing_path, "vault");
        let error = result.expect_err("unreadable keypair should abort bootstrap");

        assert!(matches!(
            error,
            StartupError::KeypairUnavailable { role: "vault", .. }
        ));
        assert_eq!(
            required_bootstrap_failure_action(),
            BootstrapFailureAction::FailFast
        );
    }
}
