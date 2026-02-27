use crate::error::AppError;
use crate::solana_types::{PROGRAM_ID, VAULT_BUFFER};
use sha2::{Digest, Sha256};
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
    rent::Rent,
    sysvar::rent,
};
use std::str::FromStr;

// ===== ANCHOR DISCRIMINATORS =====
// Computed as SHA-256("global:<instruction_name>")[0..8]

/// Discriminator for create_agreement instruction
pub const CREATE_AGREEMENT_DISCRIMINATOR: [u8; 8] = [175, 175, 109, 31, 13, 152, 155, 237];

/// Discriminator for sign_agreement instruction
pub const SIGN_AGREEMENT_DISCRIMINATOR: [u8; 8] = [134, 198, 208, 206, 148, 146, 36, 235];

/// Discriminator for cancel_agreement instruction
pub const CANCEL_AGREEMENT_DISCRIMINATOR: [u8; 8] = [124, 252, 238, 38, 238, 141, 188, 168];

/// Discriminator for expire_agreement instruction
pub const EXPIRE_AGREEMENT_DISCRIMINATOR: [u8; 8] = [252, 175, 125, 138, 29, 238, 123, 64];

/// Discriminator for vote_revoke instruction
pub const VOTE_REVOKE_DISCRIMINATOR: [u8; 8] = [167, 125, 94, 227, 160, 212, 140, 191];

/// Discriminator for retract_revoke_vote instruction
pub const RETRACT_REVOKE_VOTE_DISCRIMINATOR: [u8; 8] = [63, 165, 121, 229, 212, 114, 187, 67];

/// Discriminator for close_agreement instruction
pub const CLOSE_AGREEMENT_DISCRIMINATOR: [u8; 8] = [140, 15, 116, 49, 128, 100, 92, 236];

/// Discriminator for initialize_collection instruction
pub const INITIALIZE_COLLECTION_DISCRIMINATOR: [u8; 8] = [156, 234, 217, 40, 139, 56, 233, 159];

// ===== PDA DERIVATION =====

/// Derive agreement PDA
/// Seeds: [b"agreement", creator_pubkey, agreement_id (16 bytes)]
pub fn derive_agreement_pda(creator: &Pubkey, agreement_id: &[u8; 16]) -> (Pubkey, u8) {
    let program_id = Pubkey::from_str(PROGRAM_ID).expect("Invalid PROGRAM_ID");
    Pubkey::find_program_address(&[b"agreement", creator.as_ref(), agreement_id], &program_id)
}

/// Derive mint vault PDA
/// Seeds: [b"mint_vault", agreement_pda]
pub fn derive_mint_vault_pda(agreement: &Pubkey) -> (Pubkey, u8) {
    let program_id = Pubkey::from_str(PROGRAM_ID).expect("Invalid PROGRAM_ID");
    Pubkey::find_program_address(&[b"mint_vault", agreement.as_ref()], &program_id)
}

/// Derive PDA authority
/// Seeds: [b"mint_authority", b"v1"]
pub fn derive_pda_authority() -> (Pubkey, u8) {
    let program_id = Pubkey::from_str(PROGRAM_ID).expect("Invalid PROGRAM_ID");
    Pubkey::find_program_address(&[b"mint_authority", b"v1"], &program_id)
}

// ===== DISCRIMINATOR COMPUTATION =====

/// Compute Anchor discriminator for an instruction name
/// Formula: SHA-256("global:<name>")[0..8]
pub fn compute_discriminator(name: &str) -> [u8; 8] {
    let preimage = format!("global:{}", name);
    let hash = Sha256::digest(preimage.as_bytes());
    let mut discriminator = [0u8; 8];
    discriminator.copy_from_slice(&hash[0..8]);
    discriminator
}

// ===== INSTRUCTION BUILDER =====

/// Build an Anchor instruction
/// Combines discriminator (8 bytes) + borsh-serialized args into instruction data
pub fn build_anchor_instruction(
    program_id: &Pubkey,
    discriminator: &[u8; 8],
    accounts: Vec<AccountMeta>,
    args_data: &[u8],
) -> Instruction {
    let mut data = Vec::with_capacity(8 + args_data.len());
    data.extend_from_slice(discriminator);
    data.extend_from_slice(args_data);

    Instruction {
        program_id: *program_id,
        accounts,
        data,
    }
}

// ===== VAULT DEPOSIT CALCULATION =====

/// Calculate vault deposit amount for rent exemption + buffer
/// Returns: rent_exempt_minimum + VAULT_BUFFER
pub fn calculate_vault_deposit(agreement_state_size: usize) -> u64 {
    let rent = Rent::default();
    let rent_exempt_minimum = rent.minimum_balance(agreement_state_size);
    rent_exempt_minimum + VAULT_BUFFER
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_derive_agreement_pda_returns_valid_pubkey() {
        let creator = Pubkey::new_unique();
        let agreement_id = [1u8; 16];

        let (pda, bump) = derive_agreement_pda(&creator, &agreement_id);

        // PDA should be a valid pubkey
        assert!(pda != Pubkey::default());
        // Bump should be in valid range (0-255)
        assert!(bump <= 255);
    }

    #[test]
    fn test_derive_agreement_pda_is_deterministic() {
        let creator = Pubkey::new_unique();
        let agreement_id = [1u8; 16];

        let (pda1, bump1) = derive_agreement_pda(&creator, &agreement_id);
        let (pda2, bump2) = derive_agreement_pda(&creator, &agreement_id);

        assert_eq!(pda1, pda2, "PDA should be deterministic");
        assert_eq!(bump1, bump2, "Bump should be deterministic");
    }

    #[test]
    fn test_derive_mint_vault_pda_returns_valid_pubkey() {
        let agreement = Pubkey::new_unique();

        let (pda, bump) = derive_mint_vault_pda(&agreement);

        assert!(pda != Pubkey::default());
        assert!(bump <= 255);
    }

    #[test]
    fn test_derive_pda_authority_returns_valid_pubkey() {
        let (pda, bump) = derive_pda_authority();

        assert!(pda != Pubkey::default());
        assert!(bump <= 255);
    }

    #[test]
    fn test_compute_discriminator_returns_8_bytes() {
        let disc = compute_discriminator("create_agreement");
        assert_eq!(disc.len(), 8);
    }

    #[test]
    fn test_compute_discriminator_is_deterministic() {
        let disc1 = compute_discriminator("create_agreement");
        let disc2 = compute_discriminator("create_agreement");
        assert_eq!(disc1, disc2, "Discriminator should be deterministic");
    }

    #[test]
    fn test_compute_discriminator_different_names_produce_different_values() {
        let disc1 = compute_discriminator("create_agreement");
        let disc2 = compute_discriminator("sign_agreement");
        assert_ne!(
            disc1, disc2,
            "Different names should produce different discriminators"
        );
    }

    #[test]
    fn test_all_discriminator_constants_are_8_bytes() {
        assert_eq!(CREATE_AGREEMENT_DISCRIMINATOR.len(), 8);
        assert_eq!(SIGN_AGREEMENT_DISCRIMINATOR.len(), 8);
        assert_eq!(CANCEL_AGREEMENT_DISCRIMINATOR.len(), 8);
        assert_eq!(EXPIRE_AGREEMENT_DISCRIMINATOR.len(), 8);
        assert_eq!(VOTE_REVOKE_DISCRIMINATOR.len(), 8);
        assert_eq!(RETRACT_REVOKE_VOTE_DISCRIMINATOR.len(), 8);
        assert_eq!(CLOSE_AGREEMENT_DISCRIMINATOR.len(), 8);
        assert_eq!(INITIALIZE_COLLECTION_DISCRIMINATOR.len(), 8);
    }

    #[test]
    fn test_build_anchor_instruction_combines_discriminator_and_args() {
        let program_id = Pubkey::new_unique();
        let discriminator = [1, 2, 3, 4, 5, 6, 7, 8];
        let accounts = vec![];
        let args_data = vec![9, 10, 11, 12];

        let instruction =
            build_anchor_instruction(&program_id, &discriminator, accounts, &args_data);

        assert_eq!(instruction.program_id, program_id);
        assert_eq!(instruction.data.len(), 12); // 8 (discriminator) + 4 (args)
        assert_eq!(&instruction.data[0..8], &discriminator);
        assert_eq!(&instruction.data[8..12], &args_data);
    }

    #[test]
    fn test_calculate_vault_deposit_includes_buffer() {
        let agreement_size = 1000; // Arbitrary size
        let deposit = calculate_vault_deposit(agreement_size);

        // Deposit should be at least VAULT_BUFFER
        assert!(deposit >= VAULT_BUFFER);
        // Deposit should be reasonable (rent + buffer, not absurdly large)
        assert!(deposit < 100_000_000); // Less than 0.1 SOL
    }

    #[test]
    fn test_calculate_vault_deposit_is_deterministic() {
        let agreement_size = 1000;
        let deposit1 = calculate_vault_deposit(agreement_size);
        let deposit2 = calculate_vault_deposit(agreement_size);

        assert_eq!(
            deposit1, deposit2,
            "Vault deposit calculation should be deterministic"
        );
    }
}
