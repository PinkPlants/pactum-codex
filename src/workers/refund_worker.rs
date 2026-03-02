//! Refund Worker
//!
//! Polls `agreement_payments` every 30 seconds for rows with
//! `status = 'refund_pending'`. For each, attempts to execute the SPL token
//! transfer back to the creator's wallet, then updates the payment status.
//!
//! The actual on-chain transfer is delegated to `services::refund::execute_refund`
//! (being implemented concurrently). Until that lands, the worker runs the full
//! query loop and logs what it would do.

use std::time::Duration;

use sqlx::Row;
use uuid::Uuid;

use crate::state::AppState;

/// Entry point — spawned via `tokio::spawn(refund_worker::run(state.clone()))`.
pub async fn run(state: AppState) {
    let mut interval = tokio::time::interval(Duration::from_secs(30));
    loop {
        interval.tick().await;
        process_pending_refunds(&state).await;
    }
}

/// Query all `refund_pending` payments and attempt to execute each refund.
async fn process_pending_refunds(state: &AppState) {
    let rows = sqlx::query(
        "SELECT id, agreement_pda, token_mint, refund_amount \
         FROM agreement_payments \
         WHERE status = 'refund_pending' \
         ORDER BY refund_initiated_at ASC \
         LIMIT 10",
    )
    .fetch_all(&state.db)
    .await;

    let rows = match rows {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("refund_worker: failed to query pending refunds: {e}");
            return;
        }
    };

    if rows.is_empty() {
        return;
    }

    tracing::info!("refund_worker: processing {} pending refund(s)", rows.len());

    for row in rows {
        let id: Uuid = match row.try_get("id") {
            Ok(v) => v,
            Err(e) => {
                tracing::error!("refund_worker: failed to read payment id: {e}");
                continue;
            }
        };
        let agreement_pda: String = row.try_get("agreement_pda").unwrap_or_default();
        let token_mint: String = row.try_get("token_mint").unwrap_or_default();
        let refund_amount: i64 = row.try_get("refund_amount").unwrap_or(0);

        tracing::info!(
            "refund_worker: processing refund for payment {} (pda={}, mint={}, amount={})",
            id,
            agreement_pda,
            token_mint,
            refund_amount
        );

        // Attempt the on-chain refund transfer.
        //
        // TODO: Replace stub with real call once services/refund.rs lands:
        //   let sig = crate::services::refund::execute_refund(
        //       &state.solana,
        //       &state.treasury_keypair,
        //       &creator_pubkey,
        //       &token_mint,
        //       refund_amount as u64,
        //   ).await;
        let refund_result = execute_refund_stub(&token_mint, refund_amount).await;

        match refund_result {
            Ok(sig) => {
                tracing::info!(
                    "refund_worker: refund succeeded for payment {} — sig: {}",
                    id,
                    sig
                );

                // Update payment status to 'refunded' with the tx signature
                let now = chrono::Utc::now().timestamp();
                if let Err(e) = sqlx::query(
                    "UPDATE agreement_payments \
                     SET status = 'refunded', \
                         refund_tx_signature = $1, \
                         refund_completed_at = $2 \
                     WHERE id = $3 AND status = 'refund_pending'",
                )
                .bind(&sig)
                .bind(now)
                .bind(id)
                .execute(&state.db)
                .await
                {
                    tracing::error!(
                        "refund_worker: failed to mark payment {} as refunded: {e}",
                        id
                    );
                }
            }
            Err(e) => {
                // Log and retry next cycle — no status change so it stays refund_pending
                tracing::warn!(
                    "refund_worker: refund failed for payment {} — will retry next cycle: {e}",
                    id
                );
            }
        }
    }
}

/// MVP stub for the on-chain refund transfer.
///
/// Returns `Err` for now so the worker exercises the retry path.
/// Will be replaced by `crate::services::refund::execute_refund` once that
/// module is implemented.
async fn execute_refund_stub(token_mint: &str, amount: i64) -> Result<String, String> {
    tracing::debug!(
        "refund_worker: execute_refund_stub — would transfer {} units of {} from treasury to creator",
        amount,
        token_mint
    );
    // MVP: return error so payments stay in refund_pending for retry
    Err("refund service not yet implemented".to_string())
}
