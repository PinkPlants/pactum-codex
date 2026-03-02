//! Event Listener Worker (§12.1)
//!
//! In production, subscribes to Solana program logs via WebSocket (logsSubscribe),
//! parses Anchor instruction logs, updates PostgreSQL, and broadcasts WsEvent
//! notifications. For MVP, runs as a reconnecting stub that logs its activity.

use std::time::Duration;

use crate::state::AppState;

/// Entry point — spawned via `tokio::spawn(event_listener::run(state.clone()))`.
///
/// Outer loop implements automatic reconnection with a 5-second back-off on
/// disconnect. The inner `connect_and_listen` function will eventually hold the
/// real Solana WebSocket subscription; for MVP it sleeps and simulates a timeout.
pub async fn run(state: AppState) {
    let ws_url = &state.config.solana_ws_url;
    loop {
        match connect_and_listen(ws_url, &state).await {
            Ok(()) => {}
            Err(e) => {
                tracing::error!("Event listener disconnected: {e}; reconnecting in 5s");
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        }
    }
}

/// MVP stub — simulates a connection to the Solana WebSocket endpoint.
///
/// Real implementation will use `solana_client::pubsub_client::PubsubClient`
/// for `logsSubscribe` with the program ID filter, then dispatch each confirmed
/// log entry to `handle_confirmed_tx`.
async fn connect_and_listen(ws_url: &str, _state: &AppState) -> Result<(), String> {
    tracing::info!("event_listener: connecting to {ws_url}");

    // MVP: sleep to simulate the WebSocket connection lifetime.
    // In production this loop will block on `subscription.next()`.
    tokio::time::sleep(Duration::from_secs(60)).await;

    // Simulate a disconnect so the outer loop triggers reconnection.
    Err("connection timed out (MVP stub)".to_string())
}

/// Skeleton for handling a confirmed on-chain transaction.
///
/// Called by the real event listener when a program log is parsed into a known
/// instruction. Each arm will upsert database state, enqueue notification jobs,
/// and broadcast real-time WsEvents.
#[allow(dead_code)]
async fn handle_confirmed_tx(instruction: &str, agreement_pda: &str, _state: &AppState) {
    match instruction {
        "CreateAgreement" => {
            tracing::debug!(
                "event_listener: CreateAgreement for {agreement_pda} \
                 — would upsert parties and enqueue notifications"
            );
            // TODO: upsert_agreement_parties(state, log).await;
            // TODO: enqueue_notifications(state, NotificationEvent::AgreementCreated, log).await;
            // TODO: broadcast_ws(state, "agreement.created", log);
        }
        "SignAgreement" => {
            tracing::debug!(
                "event_listener: SignAgreement for {agreement_pda} \
                 — would update signed_at, check if fully signed, broadcast"
            );
            // TODO: update_signature(state, log).await;
            // TODO: if fully_signed { broadcast "agreement.completed" }
            // TODO: else { broadcast "agreement.signed" }
        }
        "CancelAgreement" | "ExpireAgreement" => {
            tracing::debug!(
                "event_listener: {instruction} for {agreement_pda} \
                 — would update status, notify, and initiate refund if eligible"
            );
            // TODO: update_agreement_status(state, log).await;
            // TODO: enqueue_notifications(state, NotificationEvent::Cancelled/Expired, log).await;
            // TODO: broadcast_ws(state, "agreement.cancelled"/"agreement.expired", log);
            // TODO: initiate_refund_if_eligible(state, log).await;
        }
        "VoteRevoke" => {
            tracing::debug!(
                "event_listener: VoteRevoke for {agreement_pda} \
                 — would update revoke votes and broadcast"
            );
            // TODO: update_revoke_vote(state, log).await;
            // TODO: if unanimous { broadcast "agreement.revoked" }
            // TODO: else { broadcast "agreement.revoke_vote" }
        }
        _ => {
            tracing::trace!("event_listener: unhandled instruction {instruction}");
        }
    }
}
