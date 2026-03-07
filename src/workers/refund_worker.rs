//! Refund Worker
//!
//! Polls `agreement_payments` every 30 seconds for rows with
//! `status = 'refund_pending'`. For each, attempts to execute the SPL token
//! transfer back to the creator's wallet, then updates the payment status.

use std::time::Duration;

use crate::services::refund::execute_refund;
use crate::state::AppState;
use sqlx::Row;
use uuid::Uuid;

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
    // Query payments with refund_pending status and their associated creator
    let rows = sqlx::query(
        "SELECT p.id, p.agreement_pda, p.token_mint, p.refund_amount, a.creator_pubkey \
         FROM agreement_payments p \
         JOIN agreement_parties a ON p.agreement_pda = a.agreement_pda \
         WHERE p.status = 'refund_pending' \
         AND a.creator_pubkey IS NOT NULL \
         ORDER BY p.refund_initiated_at ASC \
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
        let creator_pubkey: String = row.try_get("creator_pubkey").unwrap_or_default();

        if creator_pubkey.is_empty() {
            tracing::warn!("refund_worker: no creator found for payment {id}");
            continue;
        }

        tracing::info!(
            "refund_worker: processing refund for payment {} (pda={}, mint={}, amount={}, creator={})",
            id,
            agreement_pda,
            token_mint,
            refund_amount,
            creator_pubkey
        );

        // Execute the on-chain refund transfer
        let refund_result = execute_refund(
            &state.solana,
            &state.treasury_keypair,
            &creator_pubkey,
            &token_mint,
            refund_amount as u64,
        )
        .await;

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
                    "refund_worker: refund failed for payment {} — will retry next cycle: {:?}",
                    id,
                    e
                );
            }
        }
    }
}
