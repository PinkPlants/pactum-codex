//! Notification Worker (§12.4)
//!
//! Polls `notification_queue` every 5 seconds. For each pending job:
//! 1. Resolve `recipient_pubkey` → `user_id` (UUID)
//! 2. Attempt real-time WebSocket delivery via `send_to_user`
//! 3. Attempt email delivery via Resend API
//! 4. Mark as sent on success, increment attempts on failure

use std::time::Duration;

use crate::handlers::ws::send_to_user;
use crate::services::crypto::decrypt;
use crate::services::notification::{
    build_ws_event, fetch_pending_jobs, increment_attempts, mark_sent, send_email,
    NotificationEvent,
};
use crate::state::{AppState, ProcessHealthState};
use crate::workers::policy::{DegradationReason, WorkerStatus};
use sqlx::Row;
use uuid::Uuid;

/// Entry point — spawned via `WorkerSupervisor::spawn`.
pub async fn run(state: AppState) {
    let mut interval = tokio::time::interval(Duration::from_secs(5));
    let mut status = WorkerStatus::Healthy;

    loop {
        interval.tick().await;

        let jobs = match fetch_pending_jobs(&state.db, 10).await {
            Ok(jobs) => {
                transition_to_healthy("notification_worker", &mut status);
                jobs
            }
            Err(e) => {
                transition_to_degraded(
                    "notification_worker",
                    &mut status,
                    DegradationReason::RpcUnavailable,
                    state.process_health.as_ref(),
                );
                tracing::error!("notification_worker: failed to fetch pending jobs: {e}");
                continue;
            }
        };

        for job in jobs {
            // Resolve recipient pubkey → user UUID for WS channel lookup
            let user_id = match lookup_user_id(&state, &job.recipient_pubkey).await {
                Some(id) => id,
                None => {
                    tracing::warn!(
                        "notification_worker: no user found for pubkey {} — skipping job {}",
                        job.recipient_pubkey,
                        job.id
                    );
                    if let Err(e) = mark_sent(&state.db, job.id).await {
                        tracing::error!(
                            "notification_worker: failed to mark job {} as sent: {e}",
                            job.id
                        );
                        let _ = increment_attempts(&state.db, job.id).await;
                    }
                    continue;
                }
            };

            // Always attempt WS delivery first (instant, zero cost)
            let ws_event = build_ws_event(&job);
            let ws_delivered = send_to_user(&state, user_id, ws_event);

            if ws_delivered {
                tracing::debug!(
                    "notification_worker: WS delivered job {} to user {}",
                    job.id,
                    user_id
                );
            } else {
                tracing::debug!(
                    "notification_worker: user {} not connected — WS delivery skipped for job {}",
                    user_id,
                    job.id
                );
            }

            // Attempt email delivery
            let email_sent = match lookup_user_email(&state, &job.recipient_pubkey).await {
                Some((email, _email_nonce)) => {
                    // Parse event type
                    let event = match job.event_type.as_str() {
                        "AgreementCreated" => NotificationEvent::AgreementCreated,
                        "Signed" => NotificationEvent::Signed,
                        "Completed" => NotificationEvent::Completed,
                        "Cancelled" => NotificationEvent::Cancelled,
                        "Expired" => NotificationEvent::Expired,
                        "RevokeVote" => NotificationEvent::RevokeVote,
                        "Revoked" => NotificationEvent::Revoked,
                        "DraftReadyToSubmit" => NotificationEvent::DraftReadyToSubmit,
                        "InvitationExpired" => NotificationEvent::InvitationExpired,
                        "InvitationReminder" => NotificationEvent::InvitationReminder,
                        "PaymentConfirmed" => NotificationEvent::PaymentConfirmed,
                        "RefundInitiated" => NotificationEvent::RefundInitiated,
                        "RefundCompleted" => NotificationEvent::RefundCompleted,
                        _ => {
                            tracing::warn!(
                                "notification_worker: unknown event type {}",
                                job.event_type
                            );
                            continue;
                        }
                    };

                    match send_email(
                        &state.config.resend_api_key,
                        &state.config.email_from,
                        &email,
                        &event,
                        job.agreement_pda.as_deref(),
                    )
                    .await
                    {
                        Ok(_) => {
                            tracing::debug!(
                                "notification_worker: email sent for job {} to {}",
                                job.id,
                                email
                            );
                            true
                        }
                        Err(e) => {
                            tracing::warn!(
                                "notification_worker: email failed for job {}: {:?}",
                                job.id,
                                e
                            );
                            false
                        }
                    }
                }
                None => {
                    tracing::debug!(
                        "notification_worker: no email for user {} — email delivery skipped",
                        user_id
                    );
                    true // No email is OK, WS delivery counts
                }
            };

            // Mark job as sent if either WS or email succeeded
            if ws_delivered || email_sent {
                if let Err(e) = mark_sent(&state.db, job.id).await {
                    tracing::error!(
                        "notification_worker: failed to mark job {} as sent: {e}",
                        job.id
                    );
                    let _ = increment_attempts(&state.db, job.id).await;
                }
            } else {
                // Both failed — increment attempts for retry
                if let Err(e) = increment_attempts(&state.db, job.id).await {
                    tracing::error!(
                        "notification_worker: failed to increment attempts for job {}: {e}",
                        job.id
                    );
                }
            }
        }
    }
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

/// Resolve a wallet pubkey to the internal user UUID.
async fn lookup_user_id(state: &AppState, pubkey: &str) -> Option<Uuid> {
    let row = sqlx::query("SELECT user_id FROM auth_wallet WHERE pubkey = $1")
        .bind(pubkey)
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten()?;

    row.try_get("user_id").ok()
}

/// Resolve a wallet pubkey to the user's email (decrypted).
async fn lookup_user_email(state: &AppState, pubkey: &str) -> Option<(String, Vec<u8>)> {
    // First get user_id from pubkey
    let user_id: Uuid = sqlx::query_scalar("SELECT user_id FROM auth_wallet WHERE pubkey = $1")
        .bind(pubkey)
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten()?;

    // Then get encrypted email from user_contacts
    let row = sqlx::query(
        "SELECT email_enc, email_nonce FROM user_contacts WHERE user_id = $1 AND email_enc IS NOT NULL"
    )
    .bind(user_id)
    .fetch_optional(&state.db)
    .await
    .ok()
    .flatten()?;

    let email_enc: Vec<u8> = row.try_get("email_enc").ok()?;
    let email_nonce: Vec<u8> = row.try_get("email_nonce").ok()?;

    // Decrypt email - key is hex-encoded
    let key_bytes = hex::decode(&state.config.encryption_key).ok()?;
    let key_array: [u8; 32] = key_bytes.try_into().ok()?;
    let nonce_array: [u8; 12] = email_nonce.as_slice().try_into().ok()?;

    match decrypt(&email_enc, &nonce_array, &key_array) {
        Ok(email) => Some((email, email_nonce)),
        Err(_) => None,
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
            "notification_worker",
            &mut status,
            DegradationReason::RpcUnavailable,
            &process_health,
        );

        assert_eq!(status, WorkerStatus::Degraded);
        assert_eq!(process_health.current(), ProcessHealth::Degraded);
    }

    #[test]
    fn transition_to_healthy_restores_worker_status_after_recovery() {
        let mut status = WorkerStatus::Degraded;

        transition_to_healthy("notification_worker", &mut status);

        assert_eq!(status, WorkerStatus::Healthy);
    }
}
