//! Event Listener Worker (§12.1)
//!
//! Subscribes to Solana program logs via WebSocket (logsSubscribe),
//! parses Anchor instruction logs, updates PostgreSQL, and broadcasts WsEvent
//! notifications.

use std::time::Duration;

use crate::handlers::ws::send_to_user;
use crate::services::notification::{enqueue_notification, NotificationEvent};
use crate::services::program_log::{parse_logs, InstructionType};
use crate::services::solana_logs::SolanaLogsService;
use crate::state::{AppState, ProcessHealthState, WsEvent};
use crate::workers::policy::{DegradationReason, WorkerStatus};
use sqlx::Row;
use uuid::Uuid;

/// Entry point — spawned via `WorkerSupervisor::spawn`.
///
/// Outer loop implements automatic reconnection with a 5-second back-off on
/// disconnect.
pub async fn run(state: AppState) {
    let ws_url = &state.config.solana_ws_url;
    let program_id = &state.config.program_id;
    let mut status = WorkerStatus::Healthy;

    loop {
        match connect_and_listen(ws_url, program_id, &state, &mut status).await {
            Ok(()) => {
                transition_to_degraded(
                    "event_listener",
                    &mut status,
                    DegradationReason::RetryBackoff,
                    state.process_health.as_ref(),
                );
                tracing::info!("event_listener: connection closed, reconnecting in 5s");
            }
            Err(e) => {
                transition_to_degraded(
                    "event_listener",
                    &mut status,
                    DegradationReason::RpcUnavailable,
                    state.process_health.as_ref(),
                );
                tracing::error!("Event listener disconnected: {e}; reconnecting in 5s");
            }
        }
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}

/// Connect to Solana WebSocket and listen for program logs.
async fn connect_and_listen(
    ws_url: &str,
    program_id: &str,
    state: &AppState,
    status: &mut WorkerStatus,
) -> Result<(), String> {
    tracing::info!("event_listener: connecting to {ws_url}");

    let service = SolanaLogsService::new(ws_url, program_id)
        .map_err(|e| format!("Failed to create logs service: {e}"))?;

    let mut rx = service
        .subscribe_logs()
        .await
        .map_err(|e| format!("Failed to subscribe to logs: {e}"))?;

    tracing::info!("event_listener: subscribed to logs for program {program_id}");
    transition_to_healthy("event_listener", status);

    while let Some(log) = rx.recv().await {
        tracing::debug!(
            "event_listener: processing log for slot {} with {} entries",
            log.slot,
            log.logs.len()
        );

        if let Some(event) = parse_logs(&log.signature, log.slot, &log.logs) {
            tracing::info!(
                "event_listener: detected {:?} for agreement {}",
                event.instruction,
                event.agreement_pda
            );

            if let Err(e) = handle_event(&event, state).await {
                tracing::error!("event_listener: failed to handle event: {e}");
            }
        }
    }

    Ok(())
}

fn transition_to_degraded(
    worker: &'static str,
    status: &mut WorkerStatus,
    reason: DegradationReason,
    process_health: &ProcessHealthState,
) {
    if *status == WorkerStatus::Degraded {
        return;
    }

    *status = WorkerStatus::Degraded;
    process_health.mark_degraded();

    tracing::warn!(
        worker = worker,
        ?reason,
        status = ?WorkerStatus::Degraded,
        "worker entered degraded retry mode"
    );
}

fn transition_to_healthy(worker: &'static str, status: &mut WorkerStatus) {
    if *status == WorkerStatus::Healthy {
        return;
    }

    *status = WorkerStatus::Healthy;
    tracing::info!(worker = worker, status = ?WorkerStatus::Healthy, "worker recovered");
}

/// Handle a parsed program log event.
async fn handle_event(
    event: &crate::services::program_log::ParsedEvent,
    state: &AppState,
) -> Result<(), Box<dyn std::error::Error>> {
    match event.instruction {
        InstructionType::CreateAgreement => {
            handle_create_agreement(event, state).await?;
        }
        InstructionType::SignAgreement => {
            handle_sign_agreement(event, state).await?;
        }
        InstructionType::CancelAgreement => {
            handle_cancel_agreement(event, state).await?;
        }
        InstructionType::ExpireAgreement => {
            handle_expire_agreement(event, state).await?;
        }
        InstructionType::VoteRevoke => {
            handle_vote_revoke(event, state).await?;
        }
    }
    Ok(())
}

/// Handle CreateAgreement instruction
async fn handle_create_agreement(
    event: &crate::services::program_log::ParsedEvent,
    state: &AppState,
) -> Result<(), Box<dyn std::error::Error>> {
    let agreement_pda = &event.agreement_pda;
    let creator = event.creator.as_deref().unwrap_or("unknown");

    // Insert agreement parties into database
    for party in &event.parties {
        sqlx::query(
            r#"
            INSERT INTO agreement_parties (party_pubkey, agreement_pda, creator_pubkey, status, created_at, expires_at, title)
            VALUES ($1, $2, $3, 'PendingSignatures', extract(epoch from now()), extract(epoch from now()) + 2592000, 'Agreement')
            ON CONFLICT (party_pubkey, agreement_pda) DO NOTHING
            "#,
        )
        .bind(party)
        .bind(agreement_pda)
        .bind(creator)
        .execute(&state.db)
        .await?;
    }

    // Enqueue notifications for all parties
    for party in &event.parties {
        if let Err(e) =
            enqueue_notification(&state.db, "AgreementCreated", Some(agreement_pda), party).await
        {
            tracing::warn!("Failed to enqueue notification for {}: {e}", party);
        }
    }

    // Broadcast WebSocket event
    let ws_event = WsEvent::AgreementCreated {
        agreement_pda: agreement_pda.clone(),
    };

    for party in &event.parties {
        if let Some(user_id) = lookup_user_id(state, party).await {
            send_to_user(state, user_id, ws_event.clone());
        }
    }

    tracing::info!("event_listener: processed CreateAgreement for {agreement_pda}");
    Ok(())
}

/// Handle SignAgreement instruction
async fn handle_sign_agreement(
    event: &crate::services::program_log::ParsedEvent,
    state: &AppState,
) -> Result<(), Box<dyn std::error::Error>> {
    let agreement_pda = &event.agreement_pda;
    let signer = event.signer.as_deref().unwrap_or("unknown");

    // Update signed_at for the signing party
    let result = sqlx::query(
        r#"
        UPDATE agreement_parties 
        SET signed_at = extract(epoch from now())
        WHERE agreement_pda = $1 AND party_pubkey = $2
        RETURNING party_pubkey
        "#,
    )
    .bind(agreement_pda)
    .bind(signer)
    .fetch_optional(&state.db)
    .await?;

    if result.is_none() {
        tracing::warn!("event_listener: signer {signer} not found for agreement {agreement_pda}");
        return Ok(());
    }

    // Check if all parties have signed
    let unsigned_count: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*) FROM agreement_parties 
        WHERE agreement_pda = $1 AND signed_at IS NULL
        "#,
    )
    .bind(agreement_pda)
    .fetch_one(&state.db)
    .await?;

    let is_fully_signed = unsigned_count == 0;

    // Enqueue notification
    let event_type = if is_fully_signed {
        "Completed"
    } else {
        "Signed"
    };

    if let Err(e) = enqueue_notification(&state.db, event_type, Some(agreement_pda), signer).await {
        tracing::warn!("Failed to enqueue notification: {e}");
    }

    // Broadcast WebSocket event
    let ws_event = if is_fully_signed {
        WsEvent::AgreementCompleted {
            agreement_pda: agreement_pda.clone(),
        }
    } else {
        WsEvent::AgreementSigned {
            agreement_pda: agreement_pda.clone(),
        }
    };

    // Notify all parties
    let parties: Vec<String> =
        sqlx::query_scalar("SELECT party_pubkey FROM agreement_parties WHERE agreement_pda = $1")
            .bind(agreement_pda)
            .fetch_all(&state.db)
            .await?;

    for party in parties {
        if let Some(user_id) = lookup_user_id(state, &party).await {
            send_to_user(state, user_id, ws_event.clone());
        }
    }

    tracing::info!("event_listener: processed SignAgreement for {agreement_pda} (fully_signed={is_fully_signed})");
    Ok(())
}

/// Handle CancelAgreement instruction
async fn handle_cancel_agreement(
    event: &crate::services::program_log::ParsedEvent,
    state: &AppState,
) -> Result<(), Box<dyn std::error::Error>> {
    handle_cancellation(event, state, "Cancelled").await
}

/// Handle ExpireAgreement instruction
async fn handle_expire_agreement(
    event: &crate::services::program_log::ParsedEvent,
    state: &AppState,
) -> Result<(), Box<dyn std::error::Error>> {
    handle_cancellation(event, state, "Expired").await
}

/// Common handler for cancellation events
async fn handle_cancellation(
    event: &crate::services::program_log::ParsedEvent,
    state: &AppState,
    status: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let agreement_pda = &event.agreement_pda;

    // Update status
    sqlx::query("UPDATE agreement_parties SET status = $1 WHERE agreement_pda = $2")
        .bind(status)
        .bind(agreement_pda)
        .execute(&state.db)
        .await?;

    // Get all parties for notifications
    let parties: Vec<String> =
        sqlx::query_scalar("SELECT party_pubkey FROM agreement_parties WHERE agreement_pda = $1")
            .bind(agreement_pda)
            .fetch_all(&state.db)
            .await?;

    // Enqueue notifications
    let event_type = if status == "Cancelled" {
        "Cancelled"
    } else {
        "Expired"
    };

    for party in &parties {
        if let Err(e) =
            enqueue_notification(&state.db, event_type, Some(agreement_pda), party).await
        {
            tracing::warn!("Failed to enqueue notification: {e}");
        }
    }

    // Broadcast WebSocket event
    let ws_event = if status == "Cancelled" {
        WsEvent::AgreementCancelled {
            agreement_pda: agreement_pda.clone(),
        }
    } else {
        WsEvent::AgreementExpired {
            agreement_pda: agreement_pda.clone(),
        }
    };

    for party in &parties {
        if let Some(user_id) = lookup_user_id(state, party).await {
            send_to_user(state, user_id, ws_event.clone());
        }
    }

    // Initiate refund if payment was made
    if let Err(e) = initiate_refund_if_eligible(state, agreement_pda).await {
        tracing::warn!("Failed to initiate refund for {agreement_pda}: {e}");
    }

    tracing::info!("event_listener: processed {status} for {agreement_pda}");
    Ok(())
}

/// Handle VoteRevoke instruction
async fn handle_vote_revoke(
    event: &crate::services::program_log::ParsedEvent,
    state: &AppState,
) -> Result<(), Box<dyn std::error::Error>> {
    let agreement_pda = &event.agreement_pda;
    let signer = event.signer.as_deref().unwrap_or("unknown");

    // Enqueue notification
    if let Err(e) = enqueue_notification(&state.db, "RevokeVote", Some(agreement_pda), signer).await
    {
        tracing::warn!("Failed to enqueue notification: {e}");
    }

    // Broadcast WebSocket event
    let ws_event = WsEvent::AgreementRevokeVote {
        agreement_pda: agreement_pda.clone(),
    };

    // Notify all parties
    let parties: Vec<String> =
        sqlx::query_scalar("SELECT party_pubkey FROM agreement_parties WHERE agreement_pda = $1")
            .bind(agreement_pda)
            .fetch_all(&state.db)
            .await?;

    for party in parties {
        if let Some(user_id) = lookup_user_id(state, &party).await {
            send_to_user(state, user_id, ws_event.clone());
        }
    }

    tracing::info!("event_listener: processed VoteRevoke for {agreement_pda}");
    Ok(())
}

/// Initiate refund for cancelled/expired agreement if payment was made
async fn initiate_refund_if_eligible(
    state: &AppState,
    agreement_pda: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    // Find payment for this agreement
    let payment_id: Option<Uuid> = sqlx::query_scalar(
        r#"
        SELECT id FROM agreement_payments 
        WHERE agreement_pda = $1 AND status = 'confirmed'
        "#,
    )
    .bind(agreement_pda)
    .fetch_optional(&state.db)
    .await?;

    if let Some(id) = payment_id {
        // Mark for refund
        sqlx::query(
            r#"
            UPDATE agreement_payments 
            SET status = 'refund_pending',
                refund_initiated_at = extract(epoch from now())
            WHERE id = $1
            "#,
        )
        .bind(id)
        .execute(&state.db)
        .await?;

        tracing::info!("event_listener: marked payment {id} for refund");
    }

    Ok(())
}

/// Lookup user_id by wallet pubkey
async fn lookup_user_id(state: &AppState, pubkey: &str) -> Option<Uuid> {
    let result = sqlx::query("SELECT user_id FROM auth_wallet WHERE pubkey = $1")
        .bind(pubkey)
        .fetch_optional(&state.db)
        .await;

    match result {
        Ok(Some(row)) => row.try_get("user_id").ok(),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::ProcessHealth;

    #[test]
    fn transition_to_degraded_marks_shared_process_health() {
        let process_health = ProcessHealthState::new(ProcessHealth::Healthy);
        let mut status = WorkerStatus::Healthy;

        transition_to_degraded(
            "event_listener",
            &mut status,
            DegradationReason::RpcUnavailable,
            &process_health,
        );

        assert_eq!(status, WorkerStatus::Degraded);
        assert_eq!(process_health.current(), ProcessHealth::Degraded);
    }

    #[test]
    fn transition_to_healthy_restores_worker_status_after_reconnect() {
        let mut status = WorkerStatus::Degraded;

        transition_to_healthy("event_listener", &mut status);

        assert_eq!(status, WorkerStatus::Healthy);
    }
}
