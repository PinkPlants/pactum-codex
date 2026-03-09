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
    /// and sends parsed logs through the returned channel. The subscription is
    /// single-shot: it runs until the connection closes or the receiver is dropped.
    pub async fn subscribe_logs(&self) -> Result<mpsc::Receiver<ProgramLog>, SolanaLogsError> {
        let (tx, rx) = mpsc::channel::<ProgramLog>(100);
        let ws_url = self.ws_url.clone();
        let program_id = self.program_id;

        // Single-shot subscription: spawn the connection task once
        // If it fails or the channel closes, the task exits and does not retry
        tokio::spawn(async move {
            if let Err(e) = Self::connect_and_stream(&ws_url, program_id, &tx).await {
                error!("solana_logs: subscription ended with error: {e}");
            }
            // Task exits here - no reconnect loop
        });

        Ok(rx)
    }

    /// Internal method to connect and stream logs
    ///
    /// Returns Ok(()) when the channel is closed (receiver dropped) or Err on connection error.
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
    use axum::{
        extract::ws::{Message, WebSocket, WebSocketUpgrade},
        routing::get,
        Router,
    };
    use serde_json::{json, Value};
    use std::{sync::Arc, time::Duration};
    use tokio::sync::{oneshot, Mutex};

    const PROGRAM_ID: &str = "DF1cHTN9EE8Qonda1esTeYvFjmbYcoc52vDTjTMKvS1P";

    async fn spawn_mock_pubsub_server(
    ) -> (String, oneshot::Receiver<()>, tokio::task::JoinHandle<()>) {
        let (disconnected_tx, disconnected_rx) = oneshot::channel::<()>();
        let disconnected_tx = Arc::new(Mutex::new(Some(disconnected_tx)));

        let app = Router::new().route(
            "/",
            get({
                let disconnected_tx = disconnected_tx.clone();
                move |ws: WebSocketUpgrade| {
                    let disconnected_tx = disconnected_tx.clone();
                    async move {
                        ws.on_upgrade(move |socket| async move {
                            handle_pubsub_socket(socket, disconnected_tx).await;
                        })
                    }
                }
            }),
        );

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind mock ws listener");
        let addr = listener.local_addr().expect("read mock ws listener addr");

        let server_handle = tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });

        (format!("ws://{addr}/"), disconnected_rx, server_handle)
    }

    async fn handle_pubsub_socket(
        mut socket: WebSocket,
        disconnected_tx: Arc<Mutex<Option<oneshot::Sender<()>>>>,
    ) {
        let request = loop {
            match socket.recv().await {
                Some(Ok(Message::Text(request))) => break request,
                Some(Ok(_)) => continue,
                Some(Err(_)) | None => return,
            }
        };

        let request_json: Value =
            serde_json::from_str(&request).unwrap_or_else(|_| json!({ "id": 1 }));
        let request_id = request_json.get("id").cloned().unwrap_or_else(|| json!(1));

        let subscribe_response = json!({
            "jsonrpc": "2.0",
            "result": 1,
            "id": request_id,
        });

        socket
            .send(Message::Text(subscribe_response.to_string().into()))
            .await
            .expect("send subscribe response");

        let notification = json!({
            "jsonrpc": "2.0",
            "method": "logsNotification",
            "params": {
                "result": {
                    "context": { "slot": 42 },
                    "value": {
                        "signature": "4vJ9JU1bJJE96FWSJ4Pz7fCXuP8fQmCRvFfLqf1M4q7h",
                        "err": Value::Null,
                        "logs": ["Program log: test"]
                    }
                },
                "subscription": 1
            }
        });

        socket
            .send(Message::Text(notification.to_string().into()))
            .await
            .expect("send logs notification");

        let disconnected = tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                tokio::time::sleep(Duration::from_millis(50)).await;
                if socket
                    .send(Message::Text(
                        "{\"jsonrpc\":\"2.0\",\"method\":\"heartbeat\"}".into(),
                    ))
                    .await
                    .is_err()
                {
                    return true;
                }
            }
        })
        .await
        .is_ok();

        if disconnected {
            if let Some(tx) = disconnected_tx.lock().await.take() {
                let _ = tx.send(());
            }
        }
    }

    #[test]
    fn test_new_with_valid_program_id() {
        let service = SolanaLogsService::new("wss://api.devnet.solana.com", PROGRAM_ID);
        assert!(service.is_ok());
    }

    #[test]
    fn test_new_with_invalid_program_id() {
        let service = SolanaLogsService::new("wss://api.devnet.solana.com", "invalid-pubkey");
        assert!(service.is_err());
    }

    #[tokio::test]
    async fn subscription_stops_when_receiver_drops() {
        let (ws_url, disconnected_rx, server_handle) = spawn_mock_pubsub_server().await;
        let service = SolanaLogsService::new(ws_url, PROGRAM_ID).expect("create service");

        let rx = service
            .subscribe_logs()
            .await
            .expect("subscribe should start task");

        drop(rx);

        tokio::time::timeout(Duration::from_secs(3), disconnected_rx)
            .await
            .expect("service task should close websocket quickly after receiver drop")
            .expect("disconnect signal should be sent");

        server_handle.abort();
    }

    #[tokio::test]
    async fn subscription_error_returns_terminally() {
        let program_id = Pubkey::from_str(PROGRAM_ID).expect("valid program id");
        let (tx, _rx) = mpsc::channel::<ProgramLog>(1);

        let result = tokio::time::timeout(
            Duration::from_secs(3),
            SolanaLogsService::connect_and_stream("ws://127.0.0.1:1", program_id, &tx),
        )
        .await
        .expect("connection failure should return promptly (no retry loop)");

        assert!(
            matches!(result, Err(SolanaLogsError::ConnectionFailed(_))),
            "expected terminal connection failure, got: {result:?}"
        );
    }
}
