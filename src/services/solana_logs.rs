//! Solana WebSocket Logs Subscription Service
//!
//! Provides real-time subscription to Solana program logs via WebSocket.
//! Uses solana_client::pubsub_client::PubsubClient for logsSubscribe.

use solana_client::pubsub_client::PubsubClient;
use solana_client::rpc_config::{
    CommitmentConfig, RpcTransactionLogsConfig, RpcTransactionLogsFilter,
};
use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;
use thiserror::Error;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

/// Errors that can occur in the Solana logs service
#[derive(Error, Debug)]
pub enum SolanaLogsError {
    #[error("Invalid program ID: {0}")]
    InvalidProgramId(String),
    #[error("WebSocket connection failed: {0}")]
    ConnectionFailed(String),
    #[error("Subscription error: {0}")]
    SubscriptionError(String),
}

/// A parsed program log entry from Solana
#[derive(Debug, Clone)]
pub struct ProgramLog {
    pub signature: String,
    pub slot: u64,
    pub logs: Vec<String>,
}

/// Service for subscribing to Solana program logs via WebSocket
pub struct SolanaLogsService {
    ws_url: String,
    program_id: Pubkey,
}

impl SolanaLogsService {
    /// Create a new Solana logs service
    ///
    /// # Arguments
    /// * `ws_url` - WebSocket URL for Solana node (e.g., "wss://api.mainnet-beta.solana.com")
    /// * `program_id_str` - Program ID to filter logs for
    pub fn new(ws_url: impl Into<String>, program_id_str: &str) -> Result<Self, SolanaLogsError> {
        let program_id = Pubkey::from_str(program_id_str)
            .map_err(|e| SolanaLogsError::InvalidProgramId(format!("{e}")))?;

        Ok(Self {
            ws_url: ws_url.into(),
            program_id,
        })
    }

    /// Subscribe to program logs and return a channel receiver
    ///
    /// This method spawns a blocking task that maintains the WebSocket connection
    /// and sends parsed logs through the returned channel.
    pub async fn subscribe_logs(&self) -> Result<mpsc::Receiver<ProgramLog>, SolanaLogsError> {
        let (tx, rx) = mpsc::channel::<ProgramLog>(100);
        let ws_url = self.ws_url.clone();
        let program_id = self.program_id;

        tokio::spawn(async move {
            loop {
                match Self::connect_and_stream(&ws_url, program_id, &tx).await {
                    Ok(_) => {
                        info!("solana_logs: connection closed gracefully, reconnecting in 5s");
                    }
                    Err(e) => {
                        error!("solana_logs: connection error: {e}, reconnecting in 5s");
                    }
                }
                tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
            }
        });

        Ok(rx)
    }

    /// Internal method to connect and stream logs
    async fn connect_and_stream(
        ws_url: &str,
        program_id: Pubkey,
        tx: &mpsc::Sender<ProgramLog>,
    ) -> Result<(), SolanaLogsError> {
        let ws_url = ws_url.to_string();
        let tx = tx.clone();

        tokio::task::spawn_blocking(move || {
            info!("solana_logs: connecting to {ws_url}");

            let filter = RpcTransactionLogsFilter::Mentions(vec![program_id.to_string()]);
            let config = RpcTransactionLogsConfig {
                commitment: Some(CommitmentConfig::confirmed()),
            };

            let (_client, receiver) = PubsubClient::logs_subscribe(&ws_url, filter, config)
                .map_err(|e| SolanaLogsError::ConnectionFailed(format!("{e}")))?;

            info!("solana_logs: subscribed to logs for program {program_id}");

            loop {
                match receiver.recv() {
                    Ok(response) => {
                        let log = ProgramLog {
                            signature: response.value.signature,
                            slot: response.context.slot,
                            logs: response.value.logs,
                        };

                        debug!(
                            "solana_logs: received log for slot {} with {} log entries",
                            log.slot,
                            log.logs.len()
                        );

                        if tx.blocking_send(log).is_err() {
                            warn!("solana_logs: channel closed, stopping stream");
                            break;
                        }
                    }
                    Err(e) => {
                        error!("solana_logs: receive error: {e}");
                        return Err(SolanaLogsError::SubscriptionError(format!("{e}")));
                    }
                }
            }

            Ok(())
        })
        .await
        .map_err(|e| SolanaLogsError::SubscriptionError(format!("Join error: {e}")))?
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_with_valid_program_id() {
        let service = SolanaLogsService::new(
            "wss://api.devnet.solana.com",
            "DF1cHTN9EE8Qonda1esTeYvFjmbYcoc52vDTjTMKvS1P",
        );
        assert!(service.is_ok());
    }

    #[test]
    fn test_new_with_invalid_program_id() {
        let service = SolanaLogsService::new("wss://api.devnet.solana.com", "invalid-pubkey");
        assert!(service.is_err());
    }
}
