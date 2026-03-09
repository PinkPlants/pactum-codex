use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Query, State,
    },
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use serde_json::json;
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::{
    services::jwt::decode_access_token,
    state::{AppState, WsEvent},
};

const WS_ALLOWED_ORIGINS: [&str; 2] = ["https://pactum.app", "https://app.pactum.app"];

#[derive(Debug, Deserialize, Default)]
pub struct WsQuery {
    pub token: Option<String>,
}

pub async fn ws_handler(
    State(state): State<AppState>,
    ws: WebSocketUpgrade,
    headers: HeaderMap,
    Query(query): Query<WsQuery>,
) -> Response {
    if !is_allowed_origin(&headers) {
        return StatusCode::FORBIDDEN.into_response();
    }

    let Some(token) = extract_jwt_token(&headers, query.token.as_deref()) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    let claims = match decode_access_token(&token, state.config.as_ref()) {
        Ok(claims) => claims,
        Err(_) => return StatusCode::UNAUTHORIZED.into_response(),
    };

    let user_id = claims.sub;
    ws.on_upgrade(move |socket| handle_upgrade(state, user_id, socket))
}

async fn handle_upgrade(state: AppState, user_id: Uuid, socket: WebSocket) {
    let (tx, rx) = broadcast::channel(64);
    state.ws_channels.insert(user_id, tx);

    handle_ws_connection(socket, rx).await;

    remove_user_channel(&state, user_id);
}

async fn handle_ws_connection(mut socket: WebSocket, mut rx: broadcast::Receiver<WsEvent>) {
    loop {
        tokio::select! {
            incoming = socket.recv() => {
                match incoming {
                    Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
                    Some(Ok(_)) => {}
                }
            }
            event = rx.recv() => {
                match event {
                    Ok(event) => {
                        let payload = ws_event_to_json(&event).to_string();
                        if socket.send(Message::Text(payload.into())).await.is_err() {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        }
    }
}

fn ws_event_to_json(event: &WsEvent) -> serde_json::Value {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);

    match event {
        WsEvent::AgreementCreated { agreement_pda } => {
            json!({ "event": "agreement.created", "agreement_pda": agreement_pda, "timestamp": timestamp })
        }
        WsEvent::AgreementSigned { agreement_pda } => {
            json!({ "event": "agreement.signed", "agreement_pda": agreement_pda, "timestamp": timestamp })
        }
        WsEvent::AgreementCompleted { agreement_pda } => {
            json!({ "event": "agreement.completed", "agreement_pda": agreement_pda, "timestamp": timestamp })
        }
        WsEvent::AgreementCancelled { agreement_pda } => {
            json!({ "event": "agreement.cancelled", "agreement_pda": agreement_pda, "timestamp": timestamp })
        }
        WsEvent::AgreementExpired { agreement_pda } => {
            json!({ "event": "agreement.expired", "agreement_pda": agreement_pda, "timestamp": timestamp })
        }
        WsEvent::AgreementRevokeVote { agreement_pda } => {
            json!({ "event": "agreement.revoke_vote", "agreement_pda": agreement_pda, "timestamp": timestamp })
        }
        WsEvent::AgreementRevoked { agreement_pda } => {
            json!({ "event": "agreement.revoked", "agreement_pda": agreement_pda, "timestamp": timestamp })
        }
        WsEvent::DraftReady { draft_id } => {
            json!({ "event": "draft.ready_to_submit", "draft_id": draft_id, "timestamp": timestamp })
        }
        WsEvent::DraftInvitationExpired { draft_id } => {
            json!({ "event": "draft.invitation_expired", "draft_id": draft_id, "timestamp": timestamp })
        }
        WsEvent::PaymentConfirmed { draft_id } => {
            json!({ "event": "payment.confirmed", "draft_id": draft_id, "timestamp": timestamp })
        }
        WsEvent::RefundCompleted { draft_id } => {
            json!({ "event": "payment.refund_completed", "draft_id": draft_id, "timestamp": timestamp })
        }
        WsEvent::GenericNotification { message } => {
            json!({ "event": "notification.generic", "message": message, "timestamp": timestamp })
        }
    }
}

fn is_allowed_origin(headers: &HeaderMap) -> bool {
    let Some(origin) = headers.get("origin").and_then(|value| value.to_str().ok()) else {
        return false;
    };

    WS_ALLOWED_ORIGINS.contains(&origin)
}

fn extract_jwt_token(headers: &HeaderMap, query_token: Option<&str>) -> Option<String> {
    if let Some(token) = query_token.filter(|value| !value.is_empty()) {
        return Some(token.to_owned());
    }

    headers
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .map(ToOwned::to_owned)
}

pub fn send_to_user(state: &AppState, user_id: Uuid, event: WsEvent) -> bool {
    state
        .ws_channels
        .get(&user_id)
        .map(|sender| sender.send(event).is_ok())
        .unwrap_or(false)
}

pub fn send_to_users(state: &AppState, user_ids: &[Uuid], event: WsEvent) -> usize {
    user_ids
        .iter()
        .filter(|user_id| send_to_user(state, **user_id, event.clone()))
        .count()
}

fn remove_user_channel(state: &AppState, user_id: Uuid) {
    state.ws_channels.remove(&user_id);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        config::{Config, StablecoinInfo, StablecoinRegistry},
        state::{ProcessHealth, ProcessHealthState, ProtectedKeypair},
    };
    use axum::http::{HeaderMap, HeaderValue};
    use dashmap::DashMap;
    use solana_client::rpc_client::RpcClient;
    use solana_sdk::signature::Keypair;
    use sqlx::postgres::PgPoolOptions;

    fn test_config() -> Config {
        Config {
            database_url: "postgres://localhost/test".to_string(),
            solana_rpc_url: "http://localhost:8899".to_string(),
            solana_ws_url: "ws://localhost:8900".to_string(),
            program_id: "11111111111111111111111111111111".to_string(),
            jwt_secret: "test_secret_for_ws_handler_validation".to_string(),
            jwt_access_expiry_seconds: 900,
            jwt_refresh_expiry_seconds: 604800,
            encryption_key: "test_encryption_key_long_enough_for_aes".to_string(),
            encryption_index_key: "test_index_key".to_string(),
            google_client_id: "test".to_string(),
            google_client_secret: "test".to_string(),
            google_redirect_uri: "http://localhost/callback".to_string(),
            microsoft_client_id: "test".to_string(),
            microsoft_client_secret: "test".to_string(),
            microsoft_redirect_uri: "http://localhost/callback".to_string(),
            microsoft_tenant: "common".to_string(),
            resend_api_key: "test".to_string(),
            email_from: "test@example.com".to_string(),
            invite_base_url: "http://localhost".to_string(),
            invite_expiry_seconds: 604800,
            invite_reminder_after_seconds: 259200,
            platform_fee_usd_cents: 199,
            platform_fee_free_tier: 3,
            platform_nonrefundable_fee_cents: 10,
            platform_vault_pubkey: "test".to_string(),
            platform_vault_keypair_path: std::path::PathBuf::from("/tmp/test"),
            platform_treasury_pubkey: "test".to_string(),
            platform_treasury_keypair_path: std::path::PathBuf::from("/tmp/test"),
            vault_min_sol_alert: 0.5,
            vault_min_sol_circuit_breaker: 0.1,
            vault_funding_rate_limit_per_hour: 50,
            treasury_min_usdc_alert: 20000000,
            treasury_float_per_token: 50000000,
            treasury_sweep_dest: "test".to_string(),
            stablecoin_registry: StablecoinRegistry {
                usdc: StablecoinInfo {
                    symbol: "usdc",
                    mint: "test".to_string(),
                    ata: "test".to_string(),
                    decimals: 6,
                },
                usdt: StablecoinInfo {
                    symbol: "usdt",
                    mint: "test".to_string(),
                    ata: "test".to_string(),
                    decimals: 6,
                },
                pyusd: StablecoinInfo {
                    symbol: "pyusd",
                    mint: "test".to_string(),
                    ata: "test".to_string(),
                    decimals: 6,
                },
            },
            pinata_jwt: "test".to_string(),
            pinata_gateway_domain: "gateway.pinata.cloud".to_string(),
            arweave_wallet_path: std::path::PathBuf::from("/tmp/test"),
            server_port: 8080,
            server_host: "localhost".to_string(),
        }
    }

    fn test_state() -> AppState {
        AppState {
            db: PgPoolOptions::new()
                .connect_lazy("postgres://localhost/test")
                .expect("lazy pg pool should be constructible"),
            config: Arc::new(test_config()),
            solana: Arc::new(RpcClient::new("http://localhost:8899".to_string())),
            vault_keypair: Arc::new(ProtectedKeypair(Keypair::new())),
            treasury_keypair: Arc::new(ProtectedKeypair(Keypair::new())),
            ws_channels: Arc::new(DashMap::new()),
            process_health: Arc::new(ProcessHealthState::new(ProcessHealth::Healthy)),
        }
    }

    #[test]
    fn origin_validation_rejects_invalid_origin() {
        let mut headers = HeaderMap::new();
        headers.insert("origin", HeaderValue::from_static("https://evil.example"));

        assert!(!is_allowed_origin(&headers));
    }

    #[tokio::test]
    async fn send_to_user_returns_false_when_offline() {
        let state = test_state();
        let user_id = Uuid::new_v4();

        let delivered = send_to_user(
            &state,
            user_id,
            WsEvent::GenericNotification {
                message: "hello".to_string(),
            },
        );

        assert!(!delivered);
    }

    #[tokio::test]
    async fn send_to_user_returns_true_when_online_and_receiver_gets_event() {
        let state = test_state();
        let user_id = Uuid::new_v4();
        let (sender, mut receiver) = broadcast::channel(64);
        state.ws_channels.insert(user_id, sender);

        let event = WsEvent::AgreementCreated {
            agreement_pda: "agreement_123".to_string(),
        };

        let delivered = send_to_user(&state, user_id, event.clone());
        assert!(delivered);

        let received = receiver.recv().await.expect("receiver should get event");
        assert!(matches!(
            received,
            WsEvent::AgreementCreated { agreement_pda } if agreement_pda == "agreement_123"
        ));
    }

    #[tokio::test]
    async fn disconnect_cleanup_removes_channel() {
        let state = test_state();
        let user_id = Uuid::new_v4();
        let (sender, _receiver) = broadcast::channel(64);
        state.ws_channels.insert(user_id, sender);

        remove_user_channel(&state, user_id);

        assert!(state.ws_channels.get(&user_id).is_none());
    }
}
