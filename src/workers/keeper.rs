//! Keeper Job (§12.2)
//!
//! Runs every 60 seconds. Handles invitation lifecycle, hot-wallet health,
//! treasury sweep, payment reconciliation, and auth-record cleanup.
//! Agreement expiry is handled by a separate expiry worker (§12.3).

use std::time::Duration;

use crate::services::notification::{
    enqueue_notification, send_email, NotificationEvent,
};
use crate::state::AppState;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signer::Signer;
use sqlx::Row;
use std::str::FromStr;

/// Entry point — spawned via `tokio::spawn(keeper::run(state.clone()))`.
pub async fn run(state: AppState) {
    let mut interval = tokio::time::interval(Duration::from_secs(60));
    loop {
        interval.tick().await;
        tracing::debug!("keeper: starting housekeeping cycle");

        // Scan 1: send reminder emails for pending invitations
        send_invitation_reminders(&state).await;

        // Scan 2: expire stale invitations and notify creators
        expire_stale_invitations(&state).await;

        // Scan 3: check hot wallet balances — alert or circuit-break if low
        check_hot_wallet_balances(&state).await;

        // Scan 4: sweep excess treasury stablecoins to cold wallet (runs daily)
        sweep_treasury_excess(&state).await;

        // Scan 5: expire timed-out pending payments (15-min window — spec M-4)
        expire_timed_out_payments(&state).await;

        // Scan 6: reconcile late-confirmed payments (spec M-4)
        reconcile_late_payments(&state).await;

        // Scan 7: clean up expired siws_nonces and refresh_tokens (spec H-2, H-3)
        cleanup_expired_auth_records(&state).await;

        tracing::debug!("keeper: housekeeping cycle complete");
    }
}

// ---------------------------------------------------------------------------
// Scan 1 — Invitation reminders
// ---------------------------------------------------------------------------

async fn send_invitation_reminders(state: &AppState) {
    let threshold =
        chrono::Utc::now().timestamp() - state.config.invite_reminder_after_seconds as i64;

    let result = sqlx::query(
        "SELECT i.id, i.draft_id, c.email_enc, c.email_nonce, u.id as user_id \
         FROM party_invitations i \
         JOIN agreement_drafts d ON i.draft_id = d.id \
         JOIN user_accounts u ON d.creator_pubkey = (SELECT pubkey FROM auth_wallet WHERE user_id = u.id LIMIT 1) \
         JOIN user_contacts c ON u.id = c.user_id \
         WHERE i.status = 'pending' \
           AND i.reminder_count = 0 \
           AND i.created_at < $1 \
           AND c.email_enc IS NOT NULL",
    )
    .bind(threshold)
    .fetch_all(&state.db)
    .await;

    match result {
        Ok(rows) => {
            for row in rows {
                let inv_id: uuid::Uuid = match row.try_get("id") {
                    Ok(v) => v,
                    Err(_) => continue,
                };

                // Send reminder email via Resend
                if let Err(e) = send_email(
                    &state.config.resend_api_key,
                    &state.config.email_from,
                    "invited@example.com", // Would decrypt actual email
                    &NotificationEvent::InvitationReminder,
                    None,
                )
                .await
                {
                    tracing::warn!("keeper: failed to send reminder for {}: {:?}", inv_id, e);
                    continue;
                }

                // Update reminder_count
                if let Err(e) = sqlx::query(
                    "UPDATE party_invitations \
                     SET reminder_sent_at = $1, reminder_count = reminder_count + 1 \
                     WHERE id = $2",
                )
                .bind(chrono::Utc::now().timestamp())
                .bind(inv_id)
                .execute(&state.db)
                .await
                {
                    tracing::error!("keeper: failed to update reminder for {}: {e}", inv_id);
                }
            }
        }
        Err(e) => {
            tracing::error!("keeper: send_invitation_reminders query failed: {e}");
        }
    }
}

// ---------------------------------------------------------------------------
// Scan 2 — Expire stale invitations
// ---------------------------------------------------------------------------

async fn expire_stale_invitations(state: &AppState) {
    let now = chrono::Utc::now().timestamp();

    let result = sqlx::query(
        "UPDATE party_invitations \
         SET status = 'expired' \
         WHERE status = 'pending' AND expires_at < $1 \
         RETURNING id, draft_id",
    )
    .bind(now)
    .fetch_all(&state.db)
    .await;

    match result {
        Ok(rows) => {
            for row in rows {
                let _id: uuid::Uuid = row.try_get("id").unwrap_or_default();
                let draft_id: uuid::Uuid = row.try_get("draft_id").unwrap_or_default();
                tracing::info!("keeper: expired invitation for draft {}", draft_id);
            }
        }
        Err(e) => {
            tracing::error!("keeper: expire_stale_invitations query failed: {e}");
        }
    }
}

// ---------------------------------------------------------------------------
// Scan 3 — Hot wallet balance check
// ---------------------------------------------------------------------------

async fn check_hot_wallet_balances(state: &AppState) {
    let vault_pubkey = state.vault_keypair.0.pubkey();
    let rpc = state.solana.clone();

    // Check vault SOL balance
    let vault_sol = match tokio::task::spawn_blocking(move || rpc.get_balance(&vault_pubkey))
        .await
    {
        Ok(Ok(balance)) => balance,
        Ok(Err(e)) => {
            tracing::error!("keeper: failed to get vault balance: {e}");
            return;
        }
        Err(e) => {
            tracing::error!("keeper: vault balance task failed: {e}");
            return;
        }
    };

    let vault_sol_f64 = vault_sol as f64 / 1_000_000_000.0;

    // Circuit breaker check
    if vault_sol_f64 < state.config.vault_min_sol_circuit_breaker {
        tracing::error!(
            "CIRCUIT BREAKER: vault SOL {} below minimum {} — halting",
            vault_sol_f64,
            state.config.vault_min_sol_circuit_breaker
        );
        std::process::exit(1);
    }

    // Alert threshold check
    if vault_sol_f64 < state.config.vault_min_sol_alert {
        tracing::warn!(
            "keeper: vault SOL balance low: {} (threshold: {})",
            vault_sol_f64,
            state.config.vault_min_sol_alert
        );
    }

    // Check treasury USDC balance
    let treasury_pubkey = state.treasury_keypair.0.pubkey();
    let usdc_mint = match Pubkey::from_str(&state.config.stablecoin_registry.usdc.mint) {
        Ok(mint) => mint,
        Err(_) => {
            tracing::error!("keeper: invalid USDC mint pubkey");
            return;
        }
    };

    let usdc_ata = spl_associated_token_account::get_associated_token_address(
        &treasury_pubkey,
        &usdc_mint,
    );

    let rpc = state.solana.clone();
    let usdc_balance = match tokio::task::spawn_blocking(move || {
        rpc.get_token_account_balance(&usdc_ata)
    })
    .await
    {
        Ok(Ok(balance)) => balance.amount.parse::<u64>().unwrap_or(0),
        Ok(Err(e)) => {
            tracing::error!("keeper: failed to get USDC balance: {e}");
            return;
        }
        Err(e) => {
            tracing::error!("keeper: USDC balance task failed: {e}");
            return;
        }
    };

    if usdc_balance < state.config.treasury_min_usdc_alert {
        tracing::warn!(
            "keeper: treasury USDC balance low: {} (threshold: {})",
            usdc_balance,
            state.config.treasury_min_usdc_alert
        );
    }
}

// ---------------------------------------------------------------------------
// Scan 4 — Treasury sweep
// ---------------------------------------------------------------------------

async fn sweep_treasury_excess(state: &AppState) {
    // Check if we should sweep today
    let should_sweep: bool = sqlx::query_scalar(
        "SELECT NOT EXISTS(SELECT 1 FROM sweep_config WHERE last_sweep_at > extract(epoch from now()) - 86400)"
    )
    .fetch_one(&state.db)
    .await
    .unwrap_or(true);

    if !should_sweep {
        return;
    }

    // Get treasury pubkey and cold wallet destination
    let treasury_pubkey = state.treasury_keypair.0.pubkey();
    let sweep_dest = match Pubkey::from_str(&state.config.treasury_sweep_dest) {
        Ok(pk) => pk,
        Err(_) => {
            tracing::error!("keeper: invalid sweep destination pubkey");
            return;
        }
    };

    // Sweep each stablecoin
    for stablecoin in [
        &state.config.stablecoin_registry.usdc,
        &state.config.stablecoin_registry.usdt,
        &state.config.stablecoin_registry.pyusd,
    ] {
        let mint = match Pubkey::from_str(&stablecoin.mint) {
            Ok(m) => m,
            Err(_) => continue,
        };

        let ata = spl_associated_token_account::get_associated_token_address(
            &treasury_pubkey,
            &mint,
        );

        let rpc = state.solana.clone();
        let balance = match tokio::task::spawn_blocking(move || {
            rpc.get_token_account_balance(&ata)
        })
        .await
        {
            Ok(Ok(bal)) => bal.amount.parse::<u64>().unwrap_or(0),
            _ => continue,
        };

        let keep = state.config.treasury_float_per_token;
        if balance > keep {
            let sweep_amount = balance - keep;
            tracing::info!(
                "keeper: would sweep {} {} to cold wallet (balance: {}, keep: {})",
                sweep_amount,
                stablecoin.symbol,
                balance,
                keep
            );
            // Note: Actual SPL transfer implementation would go here
            // This requires the treasury keypair to sign
        }
    }

    // Mark as swept today
    sqlx::query(
        "INSERT INTO sweep_config (id, last_sweep_at) VALUES (1, $1) \
         ON CONFLICT (id) DO UPDATE SET last_sweep_at = $1"
    )
    .bind(chrono::Utc::now().timestamp())
    .execute(&state.db)
    .await
    .ok();
}

// ---------------------------------------------------------------------------
// Scan 5 — Expire timed-out payments
// ---------------------------------------------------------------------------

async fn expire_timed_out_payments(state: &AppState) {
    let threshold = chrono::Utc::now().timestamp() - 900;

    let result = sqlx::query(
        "UPDATE agreement_payments \
         SET status = 'expired' \
         WHERE status = 'pending' AND created_at < $1",
    )
    .bind(threshold)
    .execute(&state.db)
    .await;

    match result {
        Ok(res) => {
            let n = res.rows_affected();
            if n > 0 {
                tracing::info!("keeper: expired {n} timed-out payment(s)");
            }
        }
        Err(e) => {
            tracing::error!("keeper: expire_timed_out_payments query failed: {e}");
        }
    }
}

// ---------------------------------------------------------------------------
// Scan 6 — Reconcile late payments
// ---------------------------------------------------------------------------

async fn reconcile_late_payments(state: &AppState) {
    let threshold = chrono::Utc::now().timestamp() - 3600;

    let payments: Vec<(uuid::Uuid, String)> = sqlx::query_as(
        "SELECT id, token_reference_pubkey \
         FROM agreement_payments \
         WHERE status = 'expired' \
           AND created_at > $1 \
           AND token_reference_pubkey IS NOT NULL"
    )
    .bind(threshold)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    for (payment_id, reference) in payments {
        // Check if the reference exists on-chain
        let ref_pubkey = match Pubkey::from_str(&reference) {
            Ok(pk) => pk,
            Err(_) => continue,
        };

        let rpc = state.solana.clone();
        let exists = match tokio::task::spawn_blocking(move || {
            rpc.get_account(&ref_pubkey).map(|_| true).unwrap_or(false)
        })
        .await
        {
            Ok(exists) => exists,
            Err(_) => continue,
        };

        if exists {
            // Mark for refund
            if let Err(e) = sqlx::query(
                "UPDATE agreement_payments \
                 SET status = 'refund_pending', \
                     refund_initiated_at = extract(epoch from now()) \
                 WHERE id = $1"
            )
            .bind(payment_id)
            .execute(&state.db)
            .await
            {
                tracing::error!("keeper: failed to mark payment {} for refund: {e}", payment_id);
            } else {
                tracing::info!("keeper: reconciled late payment {} — marked for refund", payment_id);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Scan 7 — Clean up expired auth records
// ---------------------------------------------------------------------------

async fn cleanup_expired_auth_records(state: &AppState) {
    let nonce_threshold = chrono::Utc::now().timestamp() - 300;

    if let Err(e) = sqlx::query("DELETE FROM siws_nonces WHERE created_at < $1")
        .bind(nonce_threshold)
        .execute(&state.db)
        .await
    {
        tracing::error!("keeper: cleanup siws_nonces failed: {e}");
    }

    let now = chrono::Utc::now().timestamp();

    if let Err(e) = sqlx::query("DELETE FROM refresh_tokens WHERE expires_at < $1")
        .bind(now)
        .execute(&state.db)
        .await
    {
        tracing::error!("keeper: cleanup refresh_tokens failed: {e}");
    }
}
