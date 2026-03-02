//! Solana on-chain types mirroring the pactum program
//!
//! This module defines Rust types that correspond to the on-chain Solana program's
//! data structures, enums, and instruction arguments. These types are used for:
//! - Serialization/deserialization with borsh (matching on-chain serialization)
//! - Type safety in backend operations
//! - Validation against on-chain constants and limits

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

// ===== ON-CHAIN CONSTANTS =====

/// Maximum number of parties allowed in a single agreement
pub const MAX_PARTIES: u16 = 10;

/// Maximum expiry duration in seconds (90 days)
pub const MAX_EXPIRY_SECONDS: u64 = 7_776_000;

/// Maximum length of URI/IPFS/Arweave paths
pub const MAX_URI_LEN: u16 = 128;

/// Maximum length of agreement title
pub const MAX_TITLE_LEN: u16 = 64;

/// Buffer amount in lamports for vault operations (0.01 SOL)
pub const VAULT_BUFFER: u64 = 10_000_000;

/// Solana pactum program ID
pub const PROGRAM_ID: &str = "DF1cHTN9EE8Qonda1esTeYvFjmbYcoc52vDTjTMKvS1P";

// ===== ENUMS =====

/// Agreement lifecycle status on-chain
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
#[repr(u8)]
pub enum AgreementStatus {
    /// Initial state - parties can sign
    Draft = 0,
    /// Awaiting additional signatures
    PendingSignatures = 1,
    /// All parties signed, agreement active
    Completed = 2,
    /// Agreement revoked by authorized party
    Cancelled = 3,
    /// Agreement expired without completion
    Expired = 4,
    /// Agreement revoked after completion
    Revoked = 5,
}

/// Storage backend for agreement content
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
#[repr(u8)]
pub enum StorageBackend {
    /// IPFS storage
    Ipfs = 0,
    /// Arweave permanent storage
    Arweave = 1,
}

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct RevokeRetractEntryWire {
    pub pubkey: [u8; 32],
    pub count: u8,
}

#[derive(Debug, Clone, PartialEq, BorshSerialize, BorshDeserialize)]
pub struct AgreementStateWire {
    pub creator: [u8; 32],
    pub agreement_id: [u8; 16],
    pub content_hash: [u8; 32],
    pub title: String,
    pub storage_uri: String,
    pub storage_backend: StorageBackend,
    pub parties: Vec<[u8; 32]>,
    pub signed_by: Vec<[u8; 32]>,
    pub signed_at: Vec<i64>,
    pub status: AgreementStatus,
    pub created_at: i64,
    pub expires_at: i64,
    pub completed_at: Option<i64>,
    pub revoked_at: Option<i64>,
    pub nft_asset: Option<[u8; 32]>,
    pub collection: Option<[u8; 32]>,
    pub vault_funder: [u8; 32],
    pub revoke_votes: Vec<[u8; 32]>,
    pub revoke_retract_counts: Vec<RevokeRetractEntryWire>,
    pub bump: u8,
}

// ===== INSTRUCTION ARGUMENTS =====

/// Arguments for create_agreement instruction
///
/// This struct is serialized with borsh and sent to the on-chain program.
/// Field order is CRITICAL for borsh compatibility.
#[derive(Debug, Clone, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct CreateAgreementArgs {
    /// Unique agreement identifier (UUID)
    pub agreement_id: String,
    /// Human-readable title of the agreement
    pub title: String,
    /// Hash of agreement content (for verification)
    pub content_hash: String,
    /// URI pointing to agreement content (IPFS/Arweave)
    pub storage_uri: String,
    /// Which storage backend is used
    pub storage_backend: StorageBackend,
    /// List of party public keys that must sign
    pub parties: Vec<String>,
    /// Initial deposit to vault in lamports
    pub vault_deposit: u64,
    /// How many seconds until agreement expires
    pub expires_in_secs: u64,
}

/// Arguments for sign_agreement instruction
///
/// Minimal structure for signing - party is inferred from signer account.
#[derive(Debug, Clone, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct SignAgreementArgs {
    /// Optional metadata URI (signer's metadata/proof)
    pub metadata_uri: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agreement_status_discriminators() {
        // Verify enum discriminators match on-chain values
        assert_eq!(AgreementStatus::Draft as u8, 0);
        assert_eq!(AgreementStatus::PendingSignatures as u8, 1);
        assert_eq!(AgreementStatus::Completed as u8, 2);
        assert_eq!(AgreementStatus::Cancelled as u8, 3);
        assert_eq!(AgreementStatus::Expired as u8, 4);
        assert_eq!(AgreementStatus::Revoked as u8, 5);
    }

    #[test]
    fn test_storage_backend_discriminators() {
        // Verify enum discriminators match on-chain values
        assert_eq!(StorageBackend::Ipfs as u8, 0);
        assert_eq!(StorageBackend::Arweave as u8, 1);
    }

    #[test]
    fn test_create_agreement_args_borsh_roundtrip() {
        let args = CreateAgreementArgs {
            agreement_id: "550e8400-e29b-41d4-a716-446655440000".to_string(),
            title: "Partnership Agreement".to_string(),
            content_hash: "QmZTR5bc9Bg7XA3a6h5oJZp8cCcMqVuEYz7MqeZVL7aMy".to_string(),
            storage_uri: "ipfs://QmZTR5bc9Bg7XA3a6h5oJZp8cCcMqVuEYz7MqeZVL7aMy".to_string(),
            storage_backend: StorageBackend::Ipfs,
            parties: vec![
                "11111111111111111111111111111111".to_string(),
                "22222222222222222222222222222222".to_string(),
            ],
            vault_deposit: 5_000_000,
            expires_in_secs: 2_592_000, // 30 days
        };

        // Serialize
        let encoded = borsh::to_vec(&args).expect("Failed to serialize");

        // Deserialize
        let decoded: CreateAgreementArgs =
            borsh::from_slice(&encoded).expect("Failed to deserialize");

        // Verify roundtrip
        assert_eq!(args.agreement_id, decoded.agreement_id);
        assert_eq!(args.title, decoded.title);
        assert_eq!(args.content_hash, decoded.content_hash);
        assert_eq!(args.storage_uri, decoded.storage_uri);
        assert_eq!(args.storage_backend, decoded.storage_backend);
        assert_eq!(args.parties, decoded.parties);
        assert_eq!(args.vault_deposit, decoded.vault_deposit);
        assert_eq!(args.expires_in_secs, decoded.expires_in_secs);
    }

    #[test]
    fn test_sign_agreement_args_borsh_roundtrip() {
        let args_with_metadata = SignAgreementArgs {
            metadata_uri: Some("ipfs://QmExample".to_string()),
        };

        let encoded = borsh::to_vec(&args_with_metadata).expect("Failed to serialize");
        let decoded: SignAgreementArgs =
            borsh::from_slice(&encoded).expect("Failed to deserialize");

        assert_eq!(args_with_metadata.metadata_uri, decoded.metadata_uri);
    }

    #[test]
    fn test_sign_agreement_args_none_metadata() {
        let args_no_metadata = SignAgreementArgs { metadata_uri: None };

        let encoded = borsh::to_vec(&args_no_metadata).expect("Failed to serialize");
        let decoded: SignAgreementArgs =
            borsh::from_slice(&encoded).expect("Failed to deserialize");

        assert_eq!(args_no_metadata.metadata_uri, decoded.metadata_uri);
        assert!(decoded.metadata_uri.is_none());
    }

    #[test]
    fn test_constants_values() {
        // Verify constant values match specification
        assert_eq!(MAX_PARTIES, 10);
        assert_eq!(MAX_EXPIRY_SECONDS, 7_776_000);
        assert_eq!(MAX_URI_LEN, 128);
        assert_eq!(MAX_TITLE_LEN, 64);
        assert_eq!(VAULT_BUFFER, 10_000_000);
        assert_eq!(PROGRAM_ID, "DF1cHTN9EE8Qonda1esTeYvFjmbYcoc52vDTjTMKvS1P");
    }

    #[test]
    fn test_max_expiry_is_90_days() {
        const SECONDS_PER_DAY: u64 = 86_400;
        let max_days = MAX_EXPIRY_SECONDS / SECONDS_PER_DAY;
        assert_eq!(max_days, 90);
    }
}
