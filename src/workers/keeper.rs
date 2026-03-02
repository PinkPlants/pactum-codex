//! Keeper Job (§12.2)
//!
//! Runs every 60 seconds. Handles invitation lifecycle, hot-wallet health,
//! treasury sweep, payment reconciliation, and auth-record cleanup.
//! Agreement expiry is handled by a separate expiry worker (§12.3).

use std::time::Duration;

use crate::state::AppState;

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
// Scan 1 — Invitation reminders (MVP: log stub)
// ---------------------------------------------------------------------------

/// Send reminder emails for pending invitations that are older than the
/// configured reminder threshold and haven't been reminded yet.
async fn send_invitation_reminders(state: &AppState) {
    let threshold =
        chrono::Utc::now().timestamp() - state.config.invite_reminder_after_seconds as i64;

    let result = sqlx::query(
        "SELECT id FROM party_invitations \
         WHERE status = 'pending' \
           AND reminder_count = 0 \
           AND created_at < $1",
    )
    .bind(threshold)
    .fetch_all(&state.db)
    .await;

    match result {
        Ok(rows) => {
            if !rows.is_empty() {
                tracing::info!(
                    "keeper: {} invitation(s) due for reminder — email dispatch not yet implemented",
                    rows.len()
                );
            }
            // TODO: for each row, enqueue_reminder_email and UPDATE reminder_count
        }
        Err(e) => {
            tracing::error!("keeper: send_invitation_reminders query failed: {e}");
        }
    }
}

// ---------------------------------------------------------------------------
// Scan 2 — Expire stale invitations (real SQL)
// ---------------------------------------------------------------------------

/// Mark pending invitations whose `expires_at` has passed as expired.
async fn expire_stale_invitations(state: &AppState) {
    let now = chrono::Utc::now().timestamp();

    let result = sqlx::query(
        "UPDATE party_invitations \
         SET status = 'expired' \
         WHERE status = 'pending' AND expires_at < $1",
    )
    .bind(now)
    .execute(&state.db)
    .await;

    match result {
        Ok(res) => {
            let n = res.rows_affected();
            if n > 0 {
                tracing::info!("keeper: expired {n} stale invitation(s)");
                // TODO: enqueue_invitation_expired_notification for each
                // TODO: broadcast_ws(state, "draft.invitation_expired", &inv);
            }
        }
        Err(e) => {
            tracing::error!("keeper: expire_stale_invitations query failed: {e}");
        }
    }
}

// ---------------------------------------------------------------------------
// Scan 3 — Hot wallet balance check (MVP: log stub)
// ---------------------------------------------------------------------------

/// Check vault SOL and treasury stablecoin balances.
/// Sends ops alert if below warning threshold.
/// Hard-stops the server if vault SOL falls below circuit-breaker threshold.
///
/// MVP: log-only — real implementation will call `state.solana.get_balance()`
/// (blocking RPC) via `tokio::task::spawn_blocking`.
async fn check_hot_wallet_balances(_state: &AppState) {
    tracing::debug!(
        "keeper: check_hot_wallet_balances — would verify vault SOL ≥ {} and treasury USDC ≥ {}",
        "vault_min_sol_alert",
        "treasury_min_usdc_alert"
    );
    // TODO: let vault_sol = spawn_blocking(|| state.solana.get_balance(&state.vault_keypair.0.pubkey())).await;
    // TODO: circuit-breaker check → std::process::exit(1)
    // TODO: warning alert if below threshold
    // TODO: check treasury USDC/USDT/PYUSD ATA balances
}

// ---------------------------------------------------------------------------
// Scan 4 — Treasury sweep (MVP: log stub)
// ---------------------------------------------------------------------------

/// Sweep stablecoin balances above the float threshold to cold wallet.
/// Runs once per day (tracked via `last_sweep_at` in a config table).
async fn sweep_treasury_excess(_state: &AppState) {
    tracing::debug!(
        "keeper: sweep_treasury_excess — would check daily sweep eligibility and transfer excess to cold wallet"
    );
    // TODO: if !should_sweep_today(&state.db).await { return; }
    // TODO: for each (mint, ata) in state.config.stablecoin_atas() { ... }
    // TODO: mark_swept_today(&state.db).await;
}

// ---------------------------------------------------------------------------
// Scan 5 — Expire timed-out payments (real SQL)
// ---------------------------------------------------------------------------

/// Mark pending payments older than 15 minutes (900 seconds) as expired.
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
// Scan 6 — Reconcile late payments (MVP: log stub)
// ---------------------------------------------------------------------------

/// Reconciliation scan — checks chain for payments that arrived after their
/// polling window expired. If a reference key is found on-chain, sets status
/// to `refund_pending` so the refund worker can process it.
async fn reconcile_late_payments(_state: &AppState) {
    tracing::debug!(
        "keeper: reconcile_late_payments — would scan recently-expired payments for on-chain confirmation"
    );
    // TODO: SELECT expired payments created in last hour
    // TODO: for each, check_reference_on_chain(&state.solana, &payment.token_reference_pubkey)
    // TODO: if confirmed, UPDATE status = 'refund_pending', set refund_amount and refund_initiated_at
}

// ---------------------------------------------------------------------------
// Scan 7 — Clean up expired auth records (real SQL)
// ---------------------------------------------------------------------------

/// Delete expired SIWS nonces (> 5 min old) and expired refresh tokens.
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
