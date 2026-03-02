use crate::error::AppError;
use solana_client::rpc_client::RpcClient;
use solana_sdk::{pubkey::Pubkey, signature::Keypair};
use sqlx::PgPool;

pub fn generate_payment_reference() -> Keypair {
    Keypair::new()
}

fn format_amount_6dp(amount_units: i64) -> String {
    let sign = if amount_units < 0 { "-" } else { "" };
    let abs = amount_units.abs();
    let whole = abs / 1_000_000;
    let fractional = abs % 1_000_000;

    if fractional == 0 {
        return format!("{sign}{whole}");
    }

    let mut frac = format!("{fractional:06}");
    while frac.ends_with('0') {
        frac.pop();
    }

    format!("{sign}{whole}.{frac}")
}

fn percent_encode_minimal(input: &str) -> String {
    let mut out = String::with_capacity(input.len());

    for byte in input.bytes() {
        let is_unreserved =
            byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~');
        if is_unreserved {
            out.push(byte as char);
        } else {
            out.push('%');
            out.push_str(&format!("{byte:02X}"));
        }
    }

    out
}

pub fn build_solana_pay_url(
    treasury_ata: &str,
    amount_units: i64,
    mint: &str,
    reference: &str,
    label: &str,
    memo: &str,
) -> String {
    let amount = format_amount_6dp(amount_units);
    let encoded_label = percent_encode_minimal(label);
    let encoded_memo = percent_encode_minimal(memo);

    format!(
        "solana:{treasury_ata}?amount={amount}&spl-token={mint}&reference={reference}&label={encoded_label}&memo={encoded_memo}"
    )
}

pub async fn confirm_payment_atomic(
    db: &PgPool,
    reference: &str,
    tx_signature: &str,
    token_mint: &str,
    token_amount: i64,
) -> Result<bool, AppError> {
    let row = sqlx::query(
        "UPDATE agreement_payments
         SET status = 'confirmed',
             token_tx_signature = $2,
             token_mint = $3,
             token_amount = $4,
             confirmed_at = extract(epoch from now())
         WHERE token_reference_pubkey = $1
           AND status = 'pending'
         RETURNING id",
    )
    .bind(reference)
    .bind(tx_signature)
    .bind(token_mint)
    .bind(token_amount)
    .fetch_optional(db)
    .await
    .map_err(|_| AppError::InternalError)?;

    Ok(row.is_some())
}

pub fn poll_payment_confirmation(
    _rpc: &RpcClient,
    _reference_pubkey: &Pubkey,
) -> Option<(String, String, i64)> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{StablecoinInfo, StablecoinRegistry};

    #[test]
    fn resolve_supports_all_configured_methods() {
        let registry = StablecoinRegistry {
            usdc: StablecoinInfo {
                symbol: "usdc",
                mint: "mint-usdc".to_string(),
                ata: "ata-usdc".to_string(),
                decimals: 6,
            },
            usdt: StablecoinInfo {
                symbol: "usdt",
                mint: "mint-usdt".to_string(),
                ata: "ata-usdt".to_string(),
                decimals: 6,
            },
            pyusd: StablecoinInfo {
                symbol: "pyusd",
                mint: "mint-pyusd".to_string(),
                ata: "ata-pyusd".to_string(),
                decimals: 6,
            },
        };

        assert_eq!(
            registry.resolve("usdc").map(|token| token.symbol),
            Some("usdc")
        );
        assert_eq!(
            registry.resolve("usdt").map(|token| token.symbol),
            Some("usdt")
        );
        assert_eq!(
            registry.resolve("pyusd").map(|token| token.symbol),
            Some("pyusd")
        );
        assert!(registry.resolve("unknown").is_none());
    }

    #[test]
    fn build_solana_pay_url_formats_amount_and_encodes_fields() {
        let url = build_solana_pay_url(
            "PlatformUsdcAta",
            1_990_000,
            "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v",
            "UniqueReferencePubkey",
            "Pactum Pro",
            "draft-uuid/1",
        );

        assert_eq!(
            url,
            "solana:PlatformUsdcAta?amount=1.99&spl-token=EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v&reference=UniqueReferencePubkey&label=Pactum%20Pro&memo=draft-uuid%2F1"
        );
    }

    #[test]
    fn poll_payment_confirmation_placeholder_returns_none() {
        let reference = Pubkey::new_unique();
        let rpc = RpcClient::new("http://localhost:8899".to_string());
        assert!(poll_payment_confirmation(&rpc, &reference).is_none());
    }
}
