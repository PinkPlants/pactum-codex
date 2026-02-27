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
    pub mod crypto;
    pub mod hash;
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

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    tracing::info!("Pactum backend starting...");
}
