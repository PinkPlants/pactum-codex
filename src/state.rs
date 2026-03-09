use dashmap::DashMap;
use solana_sdk::signature::Keypair;
use sqlx::postgres::PgPool;
use std::sync::atomic::{AtomicU8, Ordering};
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
    pub process_health: Arc<ProcessHealthState>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProcessHealth {
    Healthy,
    Degraded,
    RuntimeFailed,
    StartupFailed,
}

pub struct ProcessHealthState {
    state: AtomicU8,
}

impl ProcessHealthState {
    const HEALTHY: u8 = 0;
    const DEGRADED: u8 = 1;
    const RUNTIME_FAILED: u8 = 2;
    const STARTUP_FAILED: u8 = 3;

    pub fn new(initial: ProcessHealth) -> Self {
        Self {
            state: AtomicU8::new(Self::encode(initial)),
        }
    }

    pub fn current(&self) -> ProcessHealth {
        Self::decode(self.state.load(Ordering::Relaxed))
    }

    pub fn set(&self, next: ProcessHealth) {
        self.state.store(Self::encode(next), Ordering::Relaxed);
    }

    pub fn mark_degraded(&self) {
        let _ = self.state.compare_exchange(
            Self::HEALTHY,
            Self::DEGRADED,
            Ordering::Relaxed,
            Ordering::Relaxed,
        );
    }

    pub fn mark_runtime_failed(&self) {
        self.set(ProcessHealth::RuntimeFailed);
    }

    fn encode(status: ProcessHealth) -> u8 {
        match status {
            ProcessHealth::Healthy => Self::HEALTHY,
            ProcessHealth::Degraded => Self::DEGRADED,
            ProcessHealth::RuntimeFailed => Self::RUNTIME_FAILED,
            ProcessHealth::StartupFailed => Self::STARTUP_FAILED,
        }
    }

    fn decode(value: u8) -> ProcessHealth {
        match value {
            Self::HEALTHY => ProcessHealth::Healthy,
            Self::DEGRADED => ProcessHealth::Degraded,
            Self::RUNTIME_FAILED => ProcessHealth::RuntimeFailed,
            Self::STARTUP_FAILED => ProcessHealth::StartupFailed,
            _ => ProcessHealth::StartupFailed,
        }
    }
}

impl AsRef<Config> for AppState {
    fn as_ref(&self) -> &Config {
        &self.config
    }
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

    #[test]
    fn mark_degraded_does_not_override_runtime_failed() {
        let state = ProcessHealthState::new(ProcessHealth::RuntimeFailed);
        state.mark_degraded();
        assert_eq!(state.current(), ProcessHealth::RuntimeFailed);
    }

    #[test]
    fn mark_degraded_does_not_override_startup_failed() {
        let state = ProcessHealthState::new(ProcessHealth::StartupFailed);
        state.mark_degraded();
        assert_eq!(state.current(), ProcessHealth::StartupFailed);
    }
}
