//! Expiry Worker (§12.3)
//!
//! Submits `expire_agreement` transactions for agreements whose signing deadline
//! has passed. Polls PostgreSQL every 5 minutes and fires once per agreement.
//!
//! Design: The on-chain `expires_at` timestamp is the source of truth. This worker
//! notices when `now >= expires_at` and submits the transaction promptly. Since
//! signing windows are measured in days, a 5-minute scan interval introduces
//! negligible delay while keeping gas costs proportional to actual expired agreements.

use std::str::FromStr;
use std::time::Duration;

use crate::error::AppError;
use crate::services::solana::build_expire_agreement_tx;
use crate::state::AppState;
use crate::workers::policy::{DegradationReason, WorkerStatus};
use base64::Engine;
use sqlx::Row;

const SCAN_INTERVAL_SECONDS: u64 = 300; // 5 minutes

pub async fn run(state: AppState) {
    let mut interval = tokio::time::interval(Duration::from_secs(SCAN_INTERVAL_SECONDS));
    let mut status = WorkerStatus::Healthy;

    loop {
        interval.tick().await;

        if status == WorkerStatus::Disabled {
            tracing::warn!(
                "expiry_worker: disabled; skipping cycle while service remains available"
            );
            continue;
        }

        status = expire_due_agreements(&state).await;
    }
}

async fn expire_due_agreements(state: &AppState) -> WorkerStatus {
    let now = chrono::Utc::now().timestamp();

    let rows = sqlx::query(
        "SELECT DISTINCT agreement_pda, creator_pubkey \
         FROM agreement_parties \
         WHERE status = 'PendingSignatures' \
           AND expires_at < $1",
    )
    .bind(now)
    .fetch_all(&state.db)
    .await;

    let rows = match rows {
        Ok(r) => r,
        Err(e) => {
            return report_degraded(
                state,
                DegradationReason::RpcUnavailable,
                "failed to query expired agreements",
                &e,
            );
        }
    };

    if rows.is_empty() {
        return WorkerStatus::Healthy;
    }

    tracing::info!(
        "expiry_worker: found {} agreement(s) ready to expire",
        rows.len()
    );

    for row in rows {
        let agreement_pda: String = match row.try_get("agreement_pda") {
            Ok(v) => v,
            Err(e) => {
                tracing::error!("expiry_worker: failed to read agreement_pda: {e}");
                continue;
            }
        };
        let creator_pubkey: String = match row.try_get("creator_pubkey") {
            Ok(v) => v,
            Err(e) => {
                tracing::error!("expiry_worker: failed to read creator_pubkey: {e}");
                continue;
            }
        };

        match lock_agreement_for_expiry(&state.db, &agreement_pda).await {
            Ok(true) => {
                tracing::info!(
                    "expiry_worker: locked agreement {} for expiry",
                    agreement_pda
                );

                match submit_expire_transaction(state, &agreement_pda, &creator_pubkey).await {
                    Ok(_) => {
                        tracing::info!(
                            "expiry_worker: submitted expire_agreement for {}",
                            agreement_pda
                        );
                    }
                    Err(e) => {
                        report_degraded(
                            state,
                            DegradationReason::RetryBackoff,
                            &format!("expire_agreement failed for {agreement_pda}"),
                            &e,
                        );
                        if let Err(revert_err) =
                            unlock_agreement_from_expiry(&state.db, &agreement_pda).await
                        {
                            report_degraded(
                                state,
                                DegradationReason::RetryBackoff,
                                &format!("failed to revert lock for {agreement_pda}"),
                                &revert_err,
                            );
                        }
                    }
                }
            }
            Ok(false) => {
                tracing::debug!(
                    "expiry_worker: agreement {} already being processed",
                    agreement_pda
                );
            }
            Err(e) => {
                report_degraded(
                    state,
                    DegradationReason::RetryBackoff,
                    &format!("failed to lock agreement {agreement_pda}"),
                    &e,
                );
            }
        }
    }

    WorkerStatus::Healthy
}

async fn lock_agreement_for_expiry(
    db: &sqlx::PgPool,
    agreement_pda: &str,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(
        "UPDATE agreement_parties \
         SET status = 'expiring' \
         WHERE agreement_pda = $1 \
           AND status = 'PendingSignatures'",
    )
    .bind(agreement_pda)
    .execute(db)
    .await?;

    Ok(result.rows_affected() > 0)
}

async fn unlock_agreement_from_expiry(
    db: &sqlx::PgPool,
    agreement_pda: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE agreement_parties \
         SET status = 'PendingSignatures' \
         WHERE agreement_pda = $1 \
           AND status = 'expiring'",
    )
    .bind(agreement_pda)
    .execute(db)
    .await?;

    Ok(())
}

async fn submit_expire_transaction(
    state: &AppState,
    agreement_pda: &str,
    creator_pubkey: &str,
) -> Result<String, AppError> {
    use solana_sdk::pubkey::Pubkey;
    use std::str::FromStr;

    let creator = Pubkey::from_str(creator_pubkey).map_err(|_| AppError::InternalError)?;
    let pda = Pubkey::from_str(agreement_pda).map_err(|_| AppError::InternalError)?;

    let solana = state.solana.clone();
    let agreement_account = tokio::task::spawn_blocking(move || solana.get_account(&pda))
        .await
        .map_err(|_| AppError::InternalError)?
        .map_err(|_| AppError::SolanaRpcError)?;

    let agreement_id = if agreement_account.data.len() >= 48 {
        let mut id_bytes = [0u8; 16];
        id_bytes.copy_from_slice(&agreement_account.data[32..48]);
        id_bytes
    } else {
        return Err(AppError::InternalError);
    };

    let tx =
        build_expire_agreement_tx(&state.solana, &creator, &agreement_id, &state.vault_keypair)
            .await?;

    let tx_bytes = base64::engine::general_purpose::STANDARD
        .decode(&tx)
        .map_err(|_| AppError::InternalError)?;
    let transaction: solana_sdk::transaction::Transaction =
        bincode::deserialize(&tx_bytes).map_err(|_| AppError::InternalError)?;

    let sig = submit_transaction_rpc(&state.config.solana_rpc_url, &transaction).await?;

    tracing::info!(
        "expiry_worker: submitted expire transaction for agreement {} (signature: {})",
        agreement_pda,
        sig
    );

    Ok(sig.to_string())
}

async fn submit_transaction_rpc(
    rpc_url: &str,
    transaction: &solana_sdk::transaction::Transaction,
) -> Result<solana_sdk::signature::Signature, crate::error::AppError> {
    use base64::Engine;
    use solana_client::rpc_client::RpcClient;
    use solana_client::rpc_request::RpcRequest;
    use solana_sdk::signature::Signature;

    let serialized =
        bincode::serialize(transaction).map_err(|_| crate::error::AppError::InternalError)?;
    let encoded = base64::engine::general_purpose::STANDARD.encode(serialized);
    let rpc_url = rpc_url.to_string();

    let sig_str: String = tokio::task::spawn_blocking(move || {
        let rpc = RpcClient::new(rpc_url);
        rpc.send(RpcRequest::SendTransaction, serde_json::json!([encoded, {"encoding": "base64", "skipPreflight": false, "preflightCommitment": "confirmed"}]))
    })
    .await
    .map_err(|_| crate::error::AppError::InternalError)?
    .map_err(|_| crate::error::AppError::SolanaRpcError)?;

    Signature::from_str(&sig_str).map_err(|_| crate::error::AppError::InternalError)
}

fn report_degraded(
    state: &AppState,
    reason: DegradationReason,
    message: &str,
    error: &dyn std::fmt::Display,
) -> WorkerStatus {
    let status = status_for_reason(&state.process_health, reason);

    tracing::error!(
        ?status,
        ?reason,
        error = %error,
        "expiry_worker: {message}"
    );

    status
}

fn status_for_reason(
    process_health: &crate::state::ProcessHealthState,
    reason: DegradationReason,
) -> WorkerStatus {
    let status = reason.suggested_status();
    if status == WorkerStatus::Degraded {
        process_health.mark_degraded();
    }
    status
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::AppError;
    use crate::state::{ProcessHealth, ProcessHealthState};

    #[test]
    fn test_scan_interval_is_5_minutes() {
        assert_eq!(SCAN_INTERVAL_SECONDS, 300);
    }

    #[tokio::test]
    async fn submit_transaction_rpc_returns_solana_rpc_error_on_invalid_rpc_url() {
        let transaction = solana_sdk::transaction::Transaction::default();

        let result = submit_transaction_rpc("not-a-valid-url", &transaction).await;

        assert!(matches!(result, Err(AppError::SolanaRpcError)));
    }

    #[test]
    fn sql_and_rpc_failures_map_to_degraded_status() {
        assert_eq!(
            DegradationReason::RpcUnavailable.suggested_status(),
            WorkerStatus::Degraded
        );
        assert_eq!(
            DegradationReason::RetryBackoff.suggested_status(),
            WorkerStatus::Degraded
        );
    }

    #[test]
    fn degraded_status_marks_process_health_for_expiry_worker_failures() {
        let process_health = ProcessHealthState::new(ProcessHealth::Healthy);

        let status = status_for_reason(&process_health, DegradationReason::RetryBackoff);

        assert_eq!(status, WorkerStatus::Degraded);
        assert_eq!(process_health.current(), ProcessHealth::Degraded);
    }
}
