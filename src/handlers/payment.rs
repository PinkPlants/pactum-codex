use crate::error::AppError;
use crate::middleware::auth::AuthUser;
use crate::middleware::wallet_guard::AuthUserWithWallet;
use crate::state::AppState;
use axum::{
    extract::{Path, State},
    Json,
};
use serde::{Deserialize, Serialize};
use solana_sdk::signer::Signer;
use uuid::Uuid;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InitiatePaymentRequest {
    pub method: String,
}

#[derive(Debug, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum InitiatePaymentResponse {
    Free,
    Pending {
        method: String,
        token_mint: String,
        treasury_ata: String,
        amount_units: i64,
        usd_equivalent: f64,
        reference_pubkey: String,
        solana_pay_url: String,
    },
}

#[derive(Debug, Serialize)]
pub struct PaymentStatusResponse {
    pub status: String,
    pub method: Option<String>,
    pub confirmed_at: Option<i64>,
}

pub async fn initiate_payment(
    State(state): State<AppState>,
    auth: AuthUserWithWallet,
    Path(draft_id): Path<Uuid>,
    Json(req): Json<InitiatePaymentRequest>,
) -> Result<Json<InitiatePaymentResponse>, AppError> {
    // Verify draft exists and belongs to this user
    let draft_creator = sqlx::query_scalar::<_, String>(
        "SELECT creator_pubkey FROM agreement_drafts WHERE id = $1",
    )
    .bind(draft_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|_| AppError::InternalError)?
    .ok_or(AppError::NotFound)?;

    if draft_creator != auth.pubkey {
        return Err(AppError::Unauthorized);
    }

    // Check free tier
    let free_used = sqlx::query_scalar::<_, i32>(
        "SELECT free_used FROM user_agreement_counts WHERE user_id = $1",
    )
    .bind(auth.user_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|_| AppError::InternalError)?
    .unwrap_or(0);

    if free_used < state.config.platform_fee_free_tier as i32 {
        // Free tier: mark draft as paid and increment counter
        sqlx::query("UPDATE agreement_drafts SET paid = true WHERE id = $1")
            .bind(draft_id)
            .execute(&state.db)
            .await
            .map_err(|_| AppError::InternalError)?;

        sqlx::query(
            "INSERT INTO user_agreement_counts (user_id, free_used)
             VALUES ($1, 1)
             ON CONFLICT (user_id)
             DO UPDATE SET free_used = user_agreement_counts.free_used + 1",
        )
        .bind(auth.user_id)
        .execute(&state.db)
        .await
        .map_err(|_| AppError::InternalError)?;

        return Ok(Json(InitiatePaymentResponse::Free));
    }

    // Paid path: resolve stablecoin method
    let token_info = state
        .config
        .stablecoin_registry
        .resolve(&req.method)
        .ok_or(AppError::PaymentMethodUnsupported)?;

    let token_mint = token_info.mint.clone();
    let treasury_ata = token_info.ata.clone();

    // Generate reference keypair for Solana Pay
    let reference_keypair = crate::services::solana_pay::generate_payment_reference();
    let reference_pubkey = reference_keypair.pubkey().to_string();

    // Token amount from config (e.g. 1_990_000 for $1.99 at 6 decimals)
    let token_amount =
        cents_to_token_units(state.config.platform_fee_usd_cents, token_info.decimals);

    // Build Solana Pay URL
    let solana_pay_url = crate::services::solana_pay::build_solana_pay_url(
        &treasury_ata,
        token_amount,
        &token_mint,
        &reference_pubkey,
        "Pactum Pro",
        &format!("{}/1", draft_id),
    );

    // Insert payment record
    sqlx::query(
        "INSERT INTO agreement_payments (
            id, draft_id, method, creator_pubkey, token_mint,
            token_amount, token_reference_pubkey, treasury_ata, status
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 'pending')",
    )
    .bind(Uuid::new_v4())
    .bind(draft_id)
    .bind(&req.method)
    .bind(&auth.pubkey)
    .bind(&token_mint)
    .bind(token_amount)
    .bind(&reference_pubkey)
    .bind(&treasury_ata)
    .execute(&state.db)
    .await
    .map_err(|_| AppError::InternalError)?;

    let usd_equivalent = state.config.platform_fee_usd_cents as f64 / 100.0;

    Ok(Json(InitiatePaymentResponse::Pending {
        method: req.method,
        token_mint,
        treasury_ata,
        amount_units: token_amount,
        usd_equivalent,
        reference_pubkey,
        solana_pay_url,
    }))
}

pub async fn payment_status(
    State(state): State<AppState>,
    _auth: AuthUser,
    Path(draft_id): Path<Uuid>,
) -> Result<Json<PaymentStatusResponse>, AppError> {
    let row = sqlx::query_as::<_, (String, Option<String>, Option<i64>)>(
        "SELECT status, method, confirmed_at
         FROM agreement_payments
         WHERE draft_id = $1
         ORDER BY created_at DESC
         LIMIT 1",
    )
    .bind(draft_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|_| AppError::InternalError)?
    .ok_or(AppError::NotFound)?;

    Ok(Json(PaymentStatusResponse {
        status: row.0,
        method: row.1,
        confirmed_at: row.2,
    }))
}

/// Convert USD cents to token units based on decimals (e.g. 199 cents → 1_990_000 at 6 decimals)
fn cents_to_token_units(cents: u32, decimals: u8) -> i64 {
    let factor = 10i64.pow(decimals as u32);
    (cents as i64 * factor) / 100
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cents_to_token_units_6_decimals() {
        // 199 cents = $1.99 → 1_990_000 at 6 decimals
        assert_eq!(cents_to_token_units(199, 6), 1_990_000);
    }

    #[test]
    fn test_cents_to_token_units_zero() {
        assert_eq!(cents_to_token_units(0, 6), 0);
    }

    #[test]
    fn test_cents_to_token_units_100_cents() {
        // 100 cents = $1.00 → 1_000_000 at 6 decimals
        assert_eq!(cents_to_token_units(100, 6), 1_000_000);
    }

    #[test]
    fn test_free_tier_check_logic() {
        // Simulates the free tier comparison without DB
        let free_used: i32 = 2;
        let free_tier: u32 = 3;
        assert!(free_used < free_tier as i32, "Should be within free tier");

        let free_used: i32 = 3;
        assert!(!(free_used < free_tier as i32), "Should exceed free tier");
    }

    #[test]
    fn test_initiate_payment_response_free_serializes() {
        let resp = InitiatePaymentResponse::Free;
        let json = serde_json::to_value(&resp).unwrap();
        assert_eq!(json["status"], "free");
    }

    #[test]
    fn test_initiate_payment_response_pending_serializes() {
        let resp = InitiatePaymentResponse::Pending {
            method: "usdc".to_string(),
            token_mint: "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v".to_string(),
            treasury_ata: "TreasuryAta".to_string(),
            amount_units: 1_990_000,
            usd_equivalent: 1.99,
            reference_pubkey: "RefPubkey".to_string(),
            solana_pay_url: "solana:TreasuryAta?amount=1.99".to_string(),
        };
        let json = serde_json::to_value(&resp).unwrap();
        assert_eq!(json["status"], "pending");
        assert_eq!(json["method"], "usdc");
        assert_eq!(json["amount_units"], 1_990_000);
    }
}
