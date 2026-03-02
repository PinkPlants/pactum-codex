use dashmap::DashMap;
use solana_sdk::signature::Keypair;
use sqlx::postgres::PgPool;
use std::sync::Arc;
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::config::Config;
use solana_client::rpc_client::RpcClient;

/// Newtype wrapper that prevents keypair bytes from appearing in logs or debug output.
/// Solana's Keypair does not implement Debug/Display; this newtype makes that explicit
/// and adds a safe redacted display for any context where AppState might be printed.
pub struct ProtectedKeypair(pub Keypair);

impl std::fmt::Debug for ProtectedKeypair {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "ProtectedKeypair([REDACTED])")
    }
}

impl std::fmt::Display for ProtectedKeypair {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "[REDACTED]")
    }
}

/// WebSocket event types for real-time notifications to connected clients.
/// Each event is routed to specific recipients via per-user broadcast channels.
#[derive(Clone, Debug)]
pub enum WsEvent {
    /// Triggered when `create_agreement` is confirmed on-chain
    AgreementCreated {
        agreement_pda: String,
    },
    /// Triggered when `sign_agreement` is confirmed (partial signature)
    AgreementSigned {
        agreement_pda: String,
    },
    /// Triggered when `sign_agreement` is confirmed (final signature)
    AgreementCompleted {
        agreement_pda: String,
    },
    /// Triggered when `cancel_agreement` is confirmed on-chain
    AgreementCancelled {
        agreement_pda: String,
    },
    /// Triggered when `expire_agreement` is confirmed on-chain
    AgreementExpired {
        agreement_pda: String,
    },
    AgreementRevokeVote {
        agreement_pda: String,
    },
    AgreementRevoked {
        agreement_pda: String,
    },
    /// Triggered when all party pubkeys are resolved — creator must sign
    DraftReady {
        draft_id: String,
    },
    /// Triggered when invited party did not respond in time
    DraftInvitationExpired {
        draft_id: String,
    },
    PaymentConfirmed {
        draft_id: String,
    },
    RefundCompleted {
        draft_id: String,
    },
    GenericNotification {
        message: String,
    },
}

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub config: Arc<Config>,
    pub solana: Arc<RpcClient>,
    /// vault_keypair: funds MintVault + pays gas for create_agreement / expire_agreement.
    /// Holds SOL only. Low float (~1–2 SOL). Blast radius: vault float only.
    pub vault_keypair: Arc<ProtectedKeypair>,
    /// treasury_keypair: owns stablecoin ATAs; signs refund SPL transfers only.
    /// Holds stablecoins only. Swept daily. Blast radius: $50 float per token.
    pub treasury_keypair: Arc<ProtectedKeypair>,
    /// Per-user WebSocket channels — keyed by user_id.
    /// Events are routed directly to the recipient; no global broadcast.
    pub ws_channels: Arc<DashMap<Uuid, broadcast::Sender<WsEvent>>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_protected_keypair_debug_redaction() {
        let keypair = Keypair::new();
        let protected = ProtectedKeypair(keypair);
        let debug_str = format!("{:?}", protected);
        assert_eq!(debug_str, "ProtectedKeypair([REDACTED])");
        assert!(!debug_str.contains("secret"));
    }

    #[test]
    fn test_protected_keypair_display_redaction() {
        let keypair = Keypair::new();
        let protected = ProtectedKeypair(keypair);
        let display_str = format!("{}", protected);
        assert_eq!(display_str, "[REDACTED]");
    }

    #[test]
    fn test_ws_event_clone() {
        let event = WsEvent::AgreementCreated {
            agreement_pda: "test123".to_string(),
        };
        let _cloned = event.clone();
    }

    #[test]
    fn test_appstate_clone() {
        // Verify AppState is Clone by instantiating with Arc-wrapped fields
        let _: fn(AppState) -> () = |state| {
            let _ = state.clone();
        };
    }
}
