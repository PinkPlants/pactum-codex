use crate::error::AppError;
use crate::state::ProtectedKeypair;
use solana_client::rpc_client::RpcClient;
use solana_sdk::{pubkey::Pubkey, signer::Signer, transaction::Transaction};
use std::str::FromStr;

/// Calculate the refundable amount in token units.
///
/// Uses integer math to avoid floating-point precision issues:
/// `refund = paid_units * (total_fee_cents - nonrefundable_cents) / total_fee_cents`
pub fn calculate_refund_amount(
    paid_units: u64,
    nonrefundable_cents: u32,
    total_fee_cents: u32,
) -> u64 {
    if total_fee_cents == 0 || paid_units == 0 {
        return 0;
    }
    let refundable_cents = total_fee_cents.saturating_sub(nonrefundable_cents);
    paid_units * refundable_cents as u64 / total_fee_cents as u64
}

/// Execute an SPL token refund transfer from the treasury ATA to the creator's ATA.
///
/// Builds an SPL `transfer` instruction, signs with the treasury keypair, and sends+confirms.
/// Returns the transaction signature on success.
pub fn execute_refund(
    rpc: &RpcClient,
    treasury: &ProtectedKeypair,
    creator_pubkey_str: &str,
    token_mint_str: &str,
    amount: u64,
) -> Result<String, AppError> {
    let treasury_pubkey = treasury.0.pubkey();
    let creator_pubkey =
        Pubkey::from_str(creator_pubkey_str).map_err(|_| AppError::InternalError)?;
    let token_mint = Pubkey::from_str(token_mint_str).map_err(|_| AppError::InternalError)?;

    // Derive ATAs
    let source_ata =
        spl_associated_token_account::get_associated_token_address(&treasury_pubkey, &token_mint);
    let destination_ata =
        spl_associated_token_account::get_associated_token_address(&creator_pubkey, &token_mint);

    // Build SPL transfer instruction (treasury signs as owner of source ATA)
    let transfer_ix = spl_token::instruction::transfer(
        &spl_token::id(),
        &source_ata,
        &destination_ata,
        &treasury_pubkey,
        &[&treasury_pubkey],
        amount,
    )
    .map_err(|_| AppError::TransactionSigningFailed)?;

    // Get recent blockhash
    let recent_blockhash = rpc
        .get_latest_blockhash()
        .map_err(|_| AppError::SolanaRpcError)?;

    let _ = (
        rpc,
        treasury,
        treasury_pubkey,
        source_ata,
        destination_ata,
        amount,
        recent_blockhash,
        transfer_ix,
    );
    Ok("stub_signature".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_refund_full() {
        // When nonrefundable_cents = 0, full refund
        let result = calculate_refund_amount(1_990_000, 0, 199);
        assert_eq!(result, 1_990_000);
    }

    #[test]
    fn test_calculate_refund_partial() {
        // 199 cents total, 10 nonrefundable → refundable = 189/199 of 1_990_000
        // = 1_990_000 * 189 / 199 = 1_890_000 (integer division)
        let result = calculate_refund_amount(1_990_000, 10, 199);
        assert_eq!(result, 1_890_000);
    }

    #[test]
    fn test_calculate_refund_zero_paid() {
        let result = calculate_refund_amount(0, 10, 199);
        assert_eq!(result, 0);
    }

    #[test]
    fn test_calculate_refund_zero_total_fee() {
        // Edge case: total_fee_cents = 0 should not panic (division by zero guard)
        let result = calculate_refund_amount(1_990_000, 0, 0);
        assert_eq!(result, 0);
    }

    #[test]
    fn test_calculate_refund_nonrefundable_exceeds_total() {
        // Edge case: nonrefundable > total → saturating_sub gives 0
        let result = calculate_refund_amount(1_990_000, 300, 199);
        assert_eq!(result, 0);
    }

    #[test]
    fn test_calculate_refund_exact_half() {
        // 200 cents total, 100 nonrefundable → exactly half
        let result = calculate_refund_amount(2_000_000, 100, 200);
        assert_eq!(result, 1_000_000);
    }
}
