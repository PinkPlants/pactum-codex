use crate::{error::AppError, solana_types::PROGRAM_ID};
use sha2::{Digest, Sha256};
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
};

pub const AGREEMENT_STATE_SIZE: usize = 1813;
pub const VAULT_BUFFER: u64 = 10_000_000;

pub const CREATE_AGREEMENT_DISCRIMINATOR: [u8; 8] = [220, 156, 65, 172, 252, 68, 74, 233];
pub const SIGN_AGREEMENT_DISCRIMINATOR: [u8; 8] = [113, 163, 162, 101, 44, 101, 65, 204];
pub const CANCEL_AGREEMENT_DISCRIMINATOR: [u8; 8] = [75, 89, 85, 4, 100, 216, 143, 37];
pub const EXPIRE_AGREEMENT_DISCRIMINATOR: [u8; 8] = [238, 66, 118, 206, 71, 195, 75, 132];
pub const VOTE_REVOKE_DISCRIMINATOR: [u8; 8] = [37, 199, 69, 222, 97, 220, 96, 2];
pub const RETRACT_REVOKE_VOTE_DISCRIMINATOR: [u8; 8] = [221, 206, 3, 95, 171, 167, 185, 239];
pub const CLOSE_AGREEMENT_DISCRIMINATOR: [u8; 8] = [48, 34, 42, 18, 144, 209, 198, 55];
pub const INITIALIZE_COLLECTION_DISCRIMINATOR: [u8; 8] = [112, 62, 53, 139, 173, 152, 98, 93];

fn pactum_program_pubkey() -> Pubkey {
    PROGRAM_ID
        .parse::<Pubkey>()
        .expect("PROGRAM_ID must be a valid Solana public key")
}

pub fn derive_agreement_pda(creator: &Pubkey, agreement_id: &[u8; 16]) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[b"agreement", creator.as_ref(), agreement_id],
        &pactum_program_pubkey(),
    )
}

pub fn derive_mint_vault_pda(agreement: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[b"mint_vault", agreement.as_ref()],
        &pactum_program_pubkey(),
    )
}

pub fn derive_pda_authority() -> (Pubkey, u8) {
    Pubkey::find_program_address(&[b"mint_authority", b"v1"], &pactum_program_pubkey())
}

pub fn compute_discriminator(name: &str) -> [u8; 8] {
    let preimage = format!("global:{name}");
    let hash = Sha256::digest(preimage.as_bytes());
    let mut discriminator = [0u8; 8];
    discriminator.copy_from_slice(&hash[..8]);
    discriminator
}

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

pub fn calculate_vault_deposit(rpc: &RpcClient) -> Result<u64, AppError> {
    let rent_exempt = rpc
        .get_minimum_balance_for_rent_exemption(AGREEMENT_STATE_SIZE)
        .map_err(|_| AppError::InternalError)?;

    Ok(rent_exempt + VAULT_BUFFER)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn test_derive_agreement_pda_known_inputs_matches_expected_address() {
        let creator = Pubkey::from_str("11111111111111111111111111111111")
            .expect("creator pubkey should parse");
        let agreement_id: [u8; 16] = [
            0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd,
            0xee, 0xff,
        ];

        let (agreement_pda, _) = derive_agreement_pda(&creator, &agreement_id);

        assert_eq!(
            agreement_pda.to_string(),
            "7o9qRddPDf63hvEnCG1ARvcnLYGkRcKs8AZuU99p1Lbn"
        );
    }

    #[test]
    fn test_all_pda_derivations_complete_without_panicking() {
        let creator = Pubkey::new_unique();
        let agreement_id = [7u8; 16];

        let (agreement_pda, _) = derive_agreement_pda(&creator, &agreement_id);
        let (mint_vault_pda, _) = derive_mint_vault_pda(&agreement_pda);
        let (authority_pda, _) = derive_pda_authority();

        assert_ne!(agreement_pda, Pubkey::default());
        assert_ne!(mint_vault_pda, Pubkey::default());
        assert_ne!(authority_pda, Pubkey::default());
    }

    #[test]
    fn test_compute_discriminator_create_agreement_matches_expected_bytes() {
        let expected = [0xdc, 0x9c, 0x41, 0xac, 0xfc, 0x44, 0x4a, 0xe9];
        assert_eq!(compute_discriminator("create_agreement"), expected);
    }

    #[test]
    fn test_discriminator_constants_are_eight_bytes_each() {
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
    fn test_discriminator_constants_match_computed_values() {
        assert_eq!(
            CREATE_AGREEMENT_DISCRIMINATOR,
            compute_discriminator("create_agreement")
        );
        assert_eq!(
            SIGN_AGREEMENT_DISCRIMINATOR,
            compute_discriminator("sign_agreement")
        );
        assert_eq!(
            CANCEL_AGREEMENT_DISCRIMINATOR,
            compute_discriminator("cancel_agreement")
        );
        assert_eq!(
            EXPIRE_AGREEMENT_DISCRIMINATOR,
            compute_discriminator("expire_agreement")
        );
        assert_eq!(
            VOTE_REVOKE_DISCRIMINATOR,
            compute_discriminator("vote_revoke")
        );
        assert_eq!(
            RETRACT_REVOKE_VOTE_DISCRIMINATOR,
            compute_discriminator("retract_revoke_vote")
        );
        assert_eq!(
            CLOSE_AGREEMENT_DISCRIMINATOR,
            compute_discriminator("close_agreement")
        );
        assert_eq!(
            INITIALIZE_COLLECTION_DISCRIMINATOR,
            compute_discriminator("initialize_collection")
        );
    }

    #[test]
    fn test_build_anchor_instruction_builds_expected_payload() {
        let program_id = Pubkey::new_unique();
        let discriminator = CREATE_AGREEMENT_DISCRIMINATOR;
        let accounts = vec![
            AccountMeta::new(Pubkey::new_unique(), true),
            AccountMeta::new_readonly(Pubkey::new_unique(), false),
        ];
        let args_data = vec![9u8, 8, 7, 6];

        let ix =
            build_anchor_instruction(&program_id, &discriminator, accounts.clone(), &args_data);

        assert_eq!(ix.program_id, program_id);
        assert_eq!(ix.accounts, accounts);
        assert_eq!(ix.data.len(), discriminator.len() + args_data.len());
        assert_eq!(&ix.data[..8], &discriminator);
        assert_eq!(&ix.data[8..], args_data.as_slice());
    }

    #[test]
    fn test_known_mint_vault_and_authority_pdas_match_expected_addresses() {
        let agreement = Pubkey::from_str("7o9qRddPDf63hvEnCG1ARvcnLYGkRcKs8AZuU99p1Lbn")
            .expect("agreement pubkey should parse");

        let (mint_vault, _) = derive_mint_vault_pda(&agreement);
        let (authority, _) = derive_pda_authority();

        assert_eq!(
            mint_vault.to_string(),
            "6akzYD7kGU9tFvxvZCrQUns83w7cAu7r1HDzNvGavNxX"
        );
        assert_eq!(
            authority.to_string(),
            "C1Fd39YuCQMLtBGKV9NWzaYkaXBVaioR5meMdGqSoh9h"
        );
    }

    #[test]
    fn test_program_id_constant_parses() {
        let parsed = pactum_program_pubkey();
        assert_eq!(parsed.to_string(), PROGRAM_ID);
    }
}

// ===== TRANSACTION CONSTRUCTION =====

use crate::config::Config;
use crate::solana_types::CreateAgreementArgs;
use crate::state::ProtectedKeypair;
use base64::Engine;
use solana_sdk::{
    message::Message, signature::Signer, system_instruction, transaction::Transaction,
};

/// Build create_agreement transaction (partially signed by vault)
pub async fn build_create_agreement_tx(
    rpc: &RpcClient,
    args: &CreateAgreementArgs,
    creator: &Pubkey,
    vault_keypair: &ProtectedKeypair,
    _config: &Config,
) -> Result<String, AppError> {
    // 1. Derive PDAs
    let (agreement_pda, _agreement_bump) = derive_agreement_pda(creator, &args.agreement_id);
    let (mint_vault_pda, _vault_bump) = derive_mint_vault_pda(&agreement_pda);

    // 2. Calculate vault deposit
    let vault_deposit = calculate_vault_deposit(rpc)?;

    // 3. Build vault transfer instruction
    let vault_transfer_ix =
        system_instruction::transfer(&vault_keypair.0.pubkey(), &mint_vault_pda, vault_deposit);

    // 4. Build create_agreement instruction
    let create_ix = build_create_agreement_instruction(args, creator, &agreement_pda)?;

    // 5. Assemble transaction
    let recent_blockhash = rpc
        .get_latest_blockhash()
        .map_err(|_| AppError::SolanaRpcError)?;

    let mut tx = Transaction::new_unsigned(Message::new(
        &[vault_transfer_ix, create_ix],
        Some(&vault_keypair.0.pubkey()),
    ));

    // 6. Partial sign with vault keypair
    tx.try_partial_sign(&[&vault_keypair.0], recent_blockhash)
        .map_err(|_| AppError::TransactionSigningFailed)?;

    // 7. Serialize to base64
    let serialized = bincode::serialize(&tx).map_err(|_| AppError::InternalError)?;
    Ok(base64::engine::general_purpose::STANDARD.encode(serialized))
}

fn build_create_agreement_instruction(
    args: &CreateAgreementArgs,
    creator: &Pubkey,
    agreement_pda: &Pubkey,
) -> Result<Instruction, AppError> {
    use borsh::BorshSerialize;

    let program_id = pactum_program_pubkey();

    // Serialize args with borsh
    let args_data = args.try_to_vec().map_err(|_| AppError::InternalError)?;

    // Build instruction data: discriminator + args
    let mut data = Vec::with_capacity(8 + args_data.len());
    data.extend_from_slice(&CREATE_AGREEMENT_DISCRIMINATOR);
    data.extend_from_slice(&args_data);

    // Account metas (simplified - real implementation needs all accounts)
    let accounts = vec![
        AccountMeta::new(*creator, true),        // creator (signer)
        AccountMeta::new(*agreement_pda, false), // agreement PDA
        AccountMeta::new_readonly(solana_sdk::system_program::id(), false), // system_program
    ];

    Ok(Instruction {
        program_id,
        accounts,
        data,
    })
}

/// Build sign_agreement transaction (unsigned - party signs client-side)
pub async fn build_sign_agreement_tx(
    rpc: &RpcClient,
    creator: &Pubkey,
    agreement_id: &[u8; 16],
    signer: &Pubkey,
    metadata_uri: Option<String>,
) -> Result<String, AppError> {
    use crate::solana_types::SignAgreementArgs;
    use borsh::BorshSerialize;

    // 1. Derive agreement PDA
    let (agreement_pda, _) = derive_agreement_pda(creator, agreement_id);

    // 2. Build sign_agreement instruction
    let args = SignAgreementArgs { metadata_uri };
    let args_data = args.try_to_vec().map_err(|_| AppError::InternalError)?;

    let mut data = Vec::with_capacity(8 + args_data.len());
    data.extend_from_slice(&SIGN_AGREEMENT_DISCRIMINATOR);
    data.extend_from_slice(&args_data);

    let program_id = pactum_program_pubkey();

    let accounts = vec![
        AccountMeta::new(*signer, true),        // signer (party)
        AccountMeta::new(agreement_pda, false), // agreement PDA
        AccountMeta::new_readonly(solana_sdk::system_program::id(), false), // system_program
    ];

    let instruction = Instruction {
        program_id,
        accounts,
        data,
    };

    // 3. Create unsigned transaction
    let recent_blockhash = rpc
        .get_latest_blockhash()
        .map_err(|_| AppError::SolanaRpcError)?;

    let tx = Transaction::new_unsigned(Message::new(&[instruction], Some(signer)));

    // Note: recent_blockhash is included but tx is NOT signed
    let mut signed_tx = tx;
    signed_tx.message.recent_blockhash = recent_blockhash;

    // 4. Serialize to base64
    let serialized = bincode::serialize(&signed_tx).map_err(|_| AppError::InternalError)?;
    Ok(base64::engine::general_purpose::STANDARD.encode(serialized))
}

// Stubs for other TX types
pub async fn build_cancel_agreement_tx(
    _rpc: &RpcClient,
    _creator: &Pubkey,
    _agreement_id: &[u8; 16],
    _canceller: &Pubkey,
) -> Result<String, AppError> {
    // TODO: Implement cancel_agreement TX construction
    Err(AppError::NotImplemented)
}

pub async fn build_expire_agreement_tx(
    _rpc: &RpcClient,
    _creator: &Pubkey,
    _agreement_id: &[u8; 16],
) -> Result<String, AppError> {
    // TODO: Implement expire_agreement TX construction
    Err(AppError::NotImplemented)
}
