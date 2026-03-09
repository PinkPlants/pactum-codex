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
#[borsh(use_discriminant = true)]
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
#[borsh(use_discriminant = true)]
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
    pub collection: [u8; 32],
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
/// Matches IDL: agreement_id [u8;16], title String, content_hash [u8;32], storage_uri String,
/// storage_backend StorageBackend, parties Vec<Pubkey>, expires_in_secs i64
#[derive(Debug, Clone, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct CreateAgreementArgs {
    /// Unique agreement identifier (16 bytes)
    pub agreement_id: [u8; 16],
    /// Human-readable title of the agreement
    pub title: String,
    /// Hash of agreement content (32 bytes)
    pub content_hash: [u8; 32],
    /// URI pointing to agreement content (IPFS/Arweave)
    pub storage_uri: String,
    /// Which storage backend is used
    pub storage_backend: StorageBackend,
    /// List of party public keys that must sign
    pub parties: Vec<[u8; 32]>,
    /// How many seconds until agreement expires (i64 per IDL)
    pub expires_in_secs: i64,
}

/// Arguments for sign_agreement instruction
///
/// Minimal structure for signing - party is inferred from signer account.
#[derive(Debug, Clone, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct SignAgreementArgs {
    /// Optional metadata URI (signer's metadata/proof)
    pub metadata_uri: Option<String>,
}

/// Arguments for cancel_agreement instruction
///
/// No arguments required - the instruction is identified by discriminator only.
/// The creator pubkey and agreement_id are derived from the PDA.
#[derive(Debug, Clone, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct CancelAgreementArgs;

/// Arguments for expire_agreement instruction
///
/// No arguments required - the instruction is identified by discriminator only.
/// The creator pubkey and agreement_id are derived from the PDA.
#[derive(Debug, Clone, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct ExpireAgreementArgs;

/// Arguments for vote_revoke instruction
///
/// No arguments required - the instruction is identified by discriminator only.
/// The voter and nft_asset accounts are provided as instruction accounts.
#[derive(Debug, Clone, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct VoteRevokeArgs;

/// Arguments for retract_revoke_vote instruction
///
/// No arguments required - the instruction is identified by discriminator only.
/// The voter account is provided as an instruction account.
#[derive(Debug, Clone, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct RetractRevokeVoteArgs;

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
            agreement_id: [
                0x55, 0x0e, 0x84, 0x00, 0xe2, 0x9b, 0x41, 0xd4, 0xa7, 0x16, 0x44, 0x66, 0x55, 0x44,
                0x00, 0x00,
            ],
            title: "Partnership Agreement".to_string(),
            content_hash: [
                0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d,
                0x0e, 0x0f, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b,
                0x1c, 0x1d, 0x1e, 0x1f,
            ],
            storage_uri: "ipfs://QmZTR5bc9Bg7XA3a6h5oJZp8cCcMqVuEYz7MqeZVL7aMy".to_string(),
            storage_backend: StorageBackend::Ipfs,
            parties: vec![[0; 32], [1; 32]],
            expires_in_secs: 2_592_000, // 30 days
        };

        let encoded = borsh::to_vec(&args).expect("Failed to serialize");
        let decoded: CreateAgreementArgs =
            borsh::from_slice(&encoded).expect("Failed to deserialize");

        assert_eq!(args.agreement_id, decoded.agreement_id);
        assert_eq!(args.title, decoded.title);
        assert_eq!(args.content_hash, decoded.content_hash);
        assert_eq!(args.storage_uri, decoded.storage_uri);
        assert_eq!(args.storage_backend, decoded.storage_backend);
        assert_eq!(args.parties, decoded.parties);
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

    // =========================================================================
    // STARTUP FATAL PATH TESTS
    // =========================================================================
    // These tests document and guard the intentional fail-fast behaviors.
    // See: startup_fatal_path.md#static-invariants

    /// Test that PROGRAM_ID is a valid Solana public key.
    /// FAIL-FAST CATEGORY: Static Invariant
    /// If this fails, the binary is misconfigured and must not start.
    #[test]
    fn test_program_id_is_valid_pubkey() {
        use solana_sdk::pubkey::Pubkey;

        // This parses the PROGRAM_ID constant and verifies it's a valid pubkey.
        // The pactum_program_pubkey() function in services::solana uses expect()
        // to ensure startup aborts if this fails.
        let pubkey: Pubkey = PROGRAM_ID
            .parse()
            .expect("PROGRAM_ID must be a valid Solana public key - this is a static invariant");

        // Verify the pubkey matches the expected value
        assert_eq!(pubkey.to_string(), PROGRAM_ID);
    }

    /// Test that PROGRAM_ID matches the expected production value.
    /// This guards against accidental modification of the program ID.
    #[test]
    fn test_program_id_matches_expected() {
        assert_eq!(PROGRAM_ID, "DF1cHTN9EE8Qonda1esTeYvFjmbYcoc52vDTjTMKvS1P");
    }
}
