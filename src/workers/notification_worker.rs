//! Notification Worker (§12.4)
//!
//! Polls `notification_queue` every 5 seconds. For each pending job:
//! 1. Resolve `recipient_pubkey` → `user_id` (UUID)
//! 2. Attempt real-time WebSocket delivery via `send_to_user`
//! 3. Attempt email delivery (MVP: log-only stub)
//! 4. Mark as sent on success, increment attempts on failure

use std::time::Duration;

use sqlx::Row;
use uuid::Uuid;

use crate::handlers::ws::send_to_user;
use crate::services::notification::{
    build_ws_event, fetch_pending_jobs, increment_attempts, mark_sent,
};
use crate::state::AppState;

/// Entry point — spawned via `tokio::spawn(notification_worker::run(state.clone()))`.
pub async fn run(state: AppState) {
    let mut interval = tokio::time::interval(Duration::from_secs(5));
    loop {
        interval.tick().await;

        let jobs = match fetch_pending_jobs(&state.db, 10).await {
            Ok(jobs) => jobs,
            Err(e) => {
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
                        "notification_worker: no user found for pubkey {} — skipping WS for job {}",
                        job.recipient_pubkey,
                        job.id
                    );
                    // Still mark as sent — we can't deliver if the user isn't registered
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

            // Email delivery (MVP: log-only stub)
            // Real implementation will look up user contact info and call send_email
            tracing::debug!(
                "notification_worker: email dispatch stub for job {} (type: {}) — not yet implemented",
                job.id,
                job.event_type
            );

            // Mark job as sent (WS attempt counts as delivery for MVP)
            if let Err(e) = mark_sent(&state.db, job.id).await {
                tracing::error!(
                    "notification_worker: failed to mark job {} as sent: {e}",
                    job.id
                );
                let _ = increment_attempts(&state.db, job.id).await;
            }
        }
    }
}

/// Resolve a wallet pubkey to the internal user UUID.
///
/// Returns `None` if no user exists with the given pubkey (e.g. invited but
/// not yet registered), or if the query fails for any reason.
async fn lookup_user_id(state: &AppState, pubkey: &str) -> Option<Uuid> {
    let row = sqlx::query("SELECT id FROM users WHERE wallet_pubkey = $1")
        .bind(pubkey)
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten()?;

    row.try_get("id").ok()
}
