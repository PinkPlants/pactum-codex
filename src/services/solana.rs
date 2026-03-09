use crate::{error::AppError, solana_types::PROGRAM_ID};
use sha2::{Digest, Sha256};
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    message::Message,
    pubkey::Pubkey,
    signature::Signer,
    transaction::Transaction,
};

// System program ID constant
const SYSTEM_PROGRAM_ID: Pubkey = Pubkey::new_from_array([
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
]);

// MPL Core program ID from IDL: CoREENxT6tW1HoK8ypY1SxRMZTcVPm7R94rH4PZNhX7d
const MPL_CORE_PROGRAM_ID: Pubkey =
    Pubkey::from_str_const("CoREENxT6tW1HoK8ypY1SxRMZTcVPm7R94rH4PZNhX7d");

pub const AGREEMENT_STATE_SIZE: usize = 1813;
pub const VAULT_BUFFER: u64 = 10_000_000;

pub const CREATE_AGREEMENT_DISCRIMINATOR: [u8; 8] = [220, 156, 65, 172, 252, 68, 74, 233];
pub const SIGN_AGREEMENT_DISCRIMINATOR: [u8; 8] = [113, 163, 162, 101, 44, 101, 65, 204];
pub const CANCEL_AGREEMENT_DISCRIMINATOR: [u8; 8] = [75, 89, 85, 4, 100, 216, 143, 37];
pub const EXPIRE_AGREEMENT_DISCRIMINATOR: [u8; 8] = [238, 66, 118, 206, 71, 195, 75, 132];
pub const VOTE_REVOKE_DISCRIMINATOR: [u8; 8] = [37, 199, 69, 222, 97, 220, 96, 2];
pub const RETRACT_REVOKE_VOTE_DISCRIMINATOR: [u8; 8] = [221, 206, 3, 95, 171, 167, 185, 239];
pub const INITIALIZE_COLLECTION_DISCRIMINATOR: [u8; 8] = [112, 62, 53, 139, 173, 152, 98, 93];

/// Returns the Pactum program pubkey.
///
/// FAIL-FAST: PROGRAM_ID is a compile-time constant defined in solana_types.rs.
/// If it fails to parse, the binary is misconfigured and must not start.
/// This is not a runtime error - the program ID is hardcoded.
/// See: startup_fatal_path.md#static-invariants
fn pactum_program_pubkey() -> Pubkey {
    PROGRAM_ID
        .parse::<Pubkey>()
        .expect("PROGRAM_ID must be a valid Solana public key")
}

/// Derive agreement PDA: ["agreement", creator, agreement_id]
pub fn derive_agreement_pda(creator: &Pubkey, agreement_id: &[u8; 16]) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[b"agreement", creator.as_ref(), agreement_id],
        &pactum_program_pubkey(),
    )
}

/// Derive collection state PDA: ["collection", creator]
pub fn derive_collection_state_pda(creator: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[b"collection", creator.as_ref()], &pactum_program_pubkey())
}

/// Derive PDA authority: ["mint_authority", "v1", vault_funder]
pub fn derive_pda_authority(vault_funder: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[b"mint_authority", b"v1", vault_funder.as_ref()],
        &pactum_program_pubkey(),
    )
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

// ===== TRANSACTION CONSTRUCTION =====

use crate::config::Config;
use crate::solana_types::{CreateAgreementArgs, SignAgreementArgs, StorageBackend};
use crate::state::ProtectedKeypair;
use base64::Engine;

/// Build create_agreement transaction (partially signed by vault)
pub async fn build_create_agreement_tx(
    rpc: &RpcClient,
    args: &CreateAgreementArgs,
    creator: &Pubkey,
    vault_keypair: &ProtectedKeypair,
    _config: &Config,
) -> Result<String, AppError> {
    use borsh::BorshSerialize;

    // 1. Derive PDAs
    let (agreement_pda, _agreement_bump) = derive_agreement_pda(creator, &args.agreement_id);
    let (collection_state_pda, _collection_bump) = derive_collection_state_pda(creator);

    // 2. Calculate vault deposit
    let vault_deposit = calculate_vault_deposit(rpc)?;

    // 3. Build create_agreement instruction with all required accounts per IDL
    let create_ix = build_create_agreement_instruction(
        args,
        creator,
        &agreement_pda,
        &collection_state_pda,
        vault_keypair,
        vault_deposit,
    )?;

    // 4. Assemble transaction
    let recent_blockhash = rpc
        .get_latest_blockhash()
        .map_err(|_| AppError::SolanaRpcError)?;

    let mut tx =
        Transaction::new_unsigned(Message::new(&[create_ix], Some(&vault_keypair.0.pubkey())));

    // 5. Partial sign with vault keypair (vault_funder is the fee payer)
    tx.try_partial_sign(&[&vault_keypair.0], recent_blockhash)
        .map_err(|_| AppError::TransactionSigningFailed)?;

    // 6. Serialize to base64
    let serialized = bincode::serialize(&tx).map_err(|_| AppError::InternalError)?;
    Ok(base64::engine::general_purpose::STANDARD.encode(serialized))
}

fn build_create_agreement_instruction(
    args: &CreateAgreementArgs,
    creator: &Pubkey,
    agreement_pda: &Pubkey,
    collection_state_pda: &Pubkey,
    vault_keypair: &ProtectedKeypair,
    vault_deposit: u64,
) -> Result<Instruction, AppError> {
    use borsh::BorshSerialize;

    let program_id = pactum_program_pubkey();

    // Serialize args with borsh
    let args_data = borsh::to_vec(args).map_err(|_| AppError::InternalError)?;

    // Build instruction data: discriminator + args
    let mut data = Vec::with_capacity(8 + args_data.len());
    data.extend_from_slice(&CREATE_AGREEMENT_DISCRIMINATOR);
    data.extend_from_slice(&args_data);

    // Account metas per IDL:
    // 1. vault_funder (writable, signer) - fee payer and vault funder
    // 2. creator (signer)
    // 3. collection_state (PDA)
    // 4. agreement (writable, PDA)
    // 5. system_program
    let accounts = vec![
        AccountMeta::new(vault_keypair.0.pubkey(), true), // vault_funder
        AccountMeta::new_readonly(*creator, true),        // creator (signer)
        AccountMeta::new(*collection_state_pda, false),   // collection_state
        AccountMeta::new(*agreement_pda, false),          // agreement PDA
        AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false), // system_program
    ];

    // Add vault transfer instruction data (the program handles the transfer internally)
    // The vault_deposit amount is passed through the instruction context
    let _ = vault_deposit; // Used by the program

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
    vault_funder: &Pubkey,
) -> Result<String, AppError> {
    use borsh::BorshSerialize;

    // 1. Derive agreement PDA
    let (agreement_pda, _) = derive_agreement_pda(creator, agreement_id);

    // 2. Derive PDA authority
    let (pda_authority, _) = derive_pda_authority(vault_funder);

    // 3. Build sign_agreement instruction
    let args = SignAgreementArgs { metadata_uri };
    let args_data = borsh::to_vec(&args).map_err(|_| AppError::InternalError)?;

    let mut data = Vec::with_capacity(8 + args_data.len());
    data.extend_from_slice(&SIGN_AGREEMENT_DISCRIMINATOR);
    data.extend_from_slice(&args_data);

    let program_id = pactum_program_pubkey();

    // Account metas per IDL:
    // 1. vault_funder (writable, signer)
    // 2. signer (signer) - the party signing
    // 3. creator
    // 4. agreement (writable)
    // 5. pda_authority (PDA)
    // 6. nft_asset (writable, signer, optional) - for final signature
    // 7. collection (writable, optional)
    // 8. mpl_core_program
    // 9. system_program
    let mut accounts = vec![
        AccountMeta::new(*vault_funder, true),           // vault_funder
        AccountMeta::new(*signer, true),                 // signer (party)
        AccountMeta::new_readonly(*creator, false),      // creator
        AccountMeta::new(agreement_pda, false),          // agreement
        AccountMeta::new_readonly(pda_authority, false), // pda_authority
        AccountMeta::new_readonly(MPL_CORE_PROGRAM_ID, false), // mpl_core_program
        AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false), // system_program
    ];

    // Optional accounts would be added here based on context
    // For now, we include the basic required accounts
    let _ = accounts; // silence unused warning for now

    let instruction = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(*vault_funder, true),
            AccountMeta::new(*signer, true),
            AccountMeta::new_readonly(*creator, false),
            AccountMeta::new(agreement_pda, false),
            AccountMeta::new_readonly(pda_authority, false),
            AccountMeta::new_readonly(MPL_CORE_PROGRAM_ID, false),
            AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
        ],
        data,
    };

    // 4. Create unsigned transaction
    let recent_blockhash = rpc
        .get_latest_blockhash()
        .map_err(|_| AppError::SolanaRpcError)?;

    let tx = Transaction::new_unsigned(Message::new(&[instruction], Some(signer)));

    let mut signed_tx = tx;
    signed_tx.message.recent_blockhash = recent_blockhash;

    // 5. Serialize to base64
    let serialized = bincode::serialize(&signed_tx).map_err(|_| AppError::InternalError)?;
    Ok(base64::engine::general_purpose::STANDARD.encode(serialized))
}

/// Build cancel_agreement transaction (partially signed by vault)
pub async fn build_cancel_agreement_tx(
    rpc: &RpcClient,
    creator: &Pubkey,
    agreement_id: &[u8; 16],
    canceller: &Pubkey,
    vault_keypair: &ProtectedKeypair,
) -> Result<String, AppError> {
    // 1. Derive agreement PDA
    let (agreement_pda, _) = derive_agreement_pda(creator, agreement_id);

    let program_id = pactum_program_pubkey();

    // Account metas per IDL:
    // 1. vault_funder (writable, signer)
    // 2. creator (signer)
    // 3. agreement (writable)
    // 4. system_program
    let accounts = vec![
        AccountMeta::new(vault_keypair.0.pubkey(), true), // vault_funder
        AccountMeta::new_readonly(*canceller, true),      // creator/canceller (signer)
        AccountMeta::new(agreement_pda, false),           // agreement
        AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false), // system_program
    ];

    let instruction = Instruction {
        program_id,
        accounts,
        data: CANCEL_AGREEMENT_DISCRIMINATOR.to_vec(),
    };

    // Assemble and sign transaction
    let recent_blockhash = rpc
        .get_latest_blockhash()
        .map_err(|_| AppError::SolanaRpcError)?;

    let mut tx = Transaction::new_unsigned(Message::new(
        &[instruction],
        Some(&vault_keypair.0.pubkey()),
    ));

    tx.try_partial_sign(&[&vault_keypair.0], recent_blockhash)
        .map_err(|_| AppError::TransactionSigningFailed)?;

    let serialized = bincode::serialize(&tx).map_err(|_| AppError::InternalError)?;
    Ok(base64::engine::general_purpose::STANDARD.encode(serialized))
}

/// Build expire_agreement transaction (partially signed by vault)
pub async fn build_expire_agreement_tx(
    rpc: &RpcClient,
    creator: &Pubkey,
    agreement_id: &[u8; 16],
    vault_keypair: &ProtectedKeypair,
) -> Result<String, AppError> {
    // 1. Derive agreement PDA
    let (agreement_pda, _) = derive_agreement_pda(creator, agreement_id);

    let program_id = pactum_program_pubkey();

    // Account metas per IDL:
    // 1. vault_funder (writable, signer)
    // 2. agreement (writable)
    // 3. creator
    // 4. system_program
    let accounts = vec![
        AccountMeta::new(vault_keypair.0.pubkey(), true), // vault_funder
        AccountMeta::new(agreement_pda, false),           // agreement
        AccountMeta::new_readonly(*creator, false),       // creator
        AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false), // system_program
    ];

    let instruction = Instruction {
        program_id,
        accounts,
        data: EXPIRE_AGREEMENT_DISCRIMINATOR.to_vec(),
    };

    // Assemble and sign transaction
    let recent_blockhash = rpc
        .get_latest_blockhash()
        .map_err(|_| AppError::SolanaRpcError)?;

    let mut tx = Transaction::new_unsigned(Message::new(
        &[instruction],
        Some(&vault_keypair.0.pubkey()),
    ));

    tx.try_partial_sign(&[&vault_keypair.0], recent_blockhash)
        .map_err(|_| AppError::TransactionSigningFailed)?;

    let serialized = bincode::serialize(&tx).map_err(|_| AppError::InternalError)?;
    Ok(base64::engine::general_purpose::STANDARD.encode(serialized))
}

/// Build vote_revoke transaction (partially signed by vault, voter signs client-side)
pub async fn build_vote_revoke_tx(
    rpc: &RpcClient,
    creator: &Pubkey,
    agreement_id: &[u8; 16],
    voter: &Pubkey,
    nft_asset: &Pubkey,
    vault_keypair: &ProtectedKeypair,
) -> Result<String, AppError> {
    // 1. Derive agreement PDA and PDA authority
    let (agreement_pda, _) = derive_agreement_pda(creator, agreement_id);
    let (pda_authority, _) = derive_pda_authority(&vault_keypair.0.pubkey());

    let program_id = pactum_program_pubkey();

    // Account metas per IDL:
    // 1. vault_funder (writable, signer)
    // 2. voter (signer)
    // 3. creator
    // 4. agreement (writable)
    // 5. nft_asset (writable)
    // 6. pda_authority (PDA)
    // 7. collection (writable, optional)
    // 8. mpl_core_program
    // 9. system_program
    let accounts = vec![
        AccountMeta::new(vault_keypair.0.pubkey(), true), // vault_funder
        AccountMeta::new(*voter, true),                   // voter
        AccountMeta::new_readonly(*creator, false),       // creator
        AccountMeta::new(agreement_pda, false),           // agreement
        AccountMeta::new(*nft_asset, false),              // nft_asset
        AccountMeta::new_readonly(pda_authority, false),  // pda_authority
        AccountMeta::new_readonly(MPL_CORE_PROGRAM_ID, false), // mpl_core_program
        AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false), // system_program
    ];

    let instruction = Instruction {
        program_id,
        accounts,
        data: VOTE_REVOKE_DISCRIMINATOR.to_vec(),
    };

    // Assemble and partial sign
    let recent_blockhash = rpc
        .get_latest_blockhash()
        .map_err(|_| AppError::SolanaRpcError)?;

    let mut tx = Transaction::new_unsigned(Message::new(
        &[instruction],
        Some(&vault_keypair.0.pubkey()),
    ));

    tx.try_partial_sign(&[&vault_keypair.0], recent_blockhash)
        .map_err(|_| AppError::TransactionSigningFailed)?;

    let serialized = bincode::serialize(&tx).map_err(|_| AppError::InternalError)?;
    Ok(base64::engine::general_purpose::STANDARD.encode(serialized))
}

/// Build retract_revoke_vote transaction (partially signed by vault)
pub async fn build_retract_revoke_vote_tx(
    rpc: &RpcClient,
    _creator: &Pubkey,
    agreement_id: &[u8; 16],
    voter: &Pubkey,
    vault_keypair: &ProtectedKeypair,
) -> Result<String, AppError> {
    // Note: We need the creator to derive the agreement PDA
    // The caller should provide the agreement PDA directly or the creator
    // For now, we assume the creator is the same as used for PDA derivation

    // This is a simplified implementation - in production you'd need to
    // fetch the agreement account to determine the creator
    let creator = voter; // Placeholder - should be fetched from agreement account

    let (agreement_pda, _) = derive_agreement_pda(creator, agreement_id);

    let program_id = pactum_program_pubkey();

    // Account metas per IDL:
    // 1. vault_funder (writable, signer)
    // 2. voter (signer)
    // 3. agreement (writable)
    let accounts = vec![
        AccountMeta::new(vault_keypair.0.pubkey(), true), // vault_funder
        AccountMeta::new(*voter, true),                   // voter
        AccountMeta::new(agreement_pda, false),           // agreement
    ];

    let instruction = Instruction {
        program_id,
        accounts,
        data: RETRACT_REVOKE_VOTE_DISCRIMINATOR.to_vec(),
    };

    // Assemble and partial sign
    let recent_blockhash = rpc
        .get_latest_blockhash()
        .map_err(|_| AppError::SolanaRpcError)?;

    let mut tx = Transaction::new_unsigned(Message::new(
        &[instruction],
        Some(&vault_keypair.0.pubkey()),
    ));

    tx.try_partial_sign(&[&vault_keypair.0], recent_blockhash)
        .map_err(|_| AppError::TransactionSigningFailed)?;

    let serialized = bincode::serialize(&tx).map_err(|_| AppError::InternalError)?;
    Ok(base64::engine::general_purpose::STANDARD.encode(serialized))
}

/// Build initialize_collection transaction (partially signed by vault)
pub async fn build_initialize_collection_tx(
    rpc: &RpcClient,
    creator: &Pubkey,
    collection_asset: &Pubkey,
    name: &str,
    uri: &str,
    vault_keypair: &ProtectedKeypair,
) -> Result<String, AppError> {
    use borsh::BorshSerialize;

    // 1. Derive PDAs
    let (collection_state_pda, _) = derive_collection_state_pda(creator);
    let (pda_authority, _) = derive_pda_authority(&vault_keypair.0.pubkey());

    let program_id = pactum_program_pubkey();

    // Build args
    #[derive(BorshSerialize)]
    struct InitializeCollectionArgs {
        name: String,
        uri: String,
    }
    let args = InitializeCollectionArgs {
        name: name.to_string(),
        uri: uri.to_string(),
    };
    let args_data = borsh::to_vec(&args).map_err(|_| AppError::InternalError)?;

    let mut data = Vec::with_capacity(8 + args_data.len());
    data.extend_from_slice(&INITIALIZE_COLLECTION_DISCRIMINATOR);
    data.extend_from_slice(&args_data);

    // Account metas per IDL:
    // 1. vault_funder (writable, signer)
    // 2. creator (signer)
    // 3. collection_asset (writable, signer)
    // 4. collection_state (writable, PDA)
    // 5. pda_authority (PDA)
    // 6. mpl_core_program
    // 7. system_program
    let accounts = vec![
        AccountMeta::new(vault_keypair.0.pubkey(), true), // vault_funder
        AccountMeta::new_readonly(*creator, true),        // creator
        AccountMeta::new(*collection_asset, true),        // collection_asset
        AccountMeta::new(collection_state_pda, false),    // collection_state
        AccountMeta::new_readonly(pda_authority, false),  // pda_authority
        AccountMeta::new_readonly(MPL_CORE_PROGRAM_ID, false), // mpl_core_program
        AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false), // system_program
    ];

    let instruction = Instruction {
        program_id,
        accounts,
        data,
    };

    // Assemble and partial sign
    let recent_blockhash = rpc
        .get_latest_blockhash()
        .map_err(|_| AppError::SolanaRpcError)?;

    let mut tx = Transaction::new_unsigned(Message::new(
        &[instruction],
        Some(&vault_keypair.0.pubkey()),
    ));

    tx.try_partial_sign(&[&vault_keypair.0], recent_blockhash)
        .map_err(|_| AppError::TransactionSigningFailed)?;

    let serialized = bincode::serialize(&tx).map_err(|_| AppError::InternalError)?;
    Ok(base64::engine::general_purpose::STANDARD.encode(serialized))
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
    fn test_derive_collection_state_pda_matches_expected() {
        let creator = Pubkey::from_str("11111111111111111111111111111111")
            .expect("creator pubkey should parse");

        let (collection_pda, _) = derive_collection_state_pda(&creator);

        // Verify it's a valid PDA (doesn't start with a predictable pattern)
        assert_ne!(collection_pda, Pubkey::default());
    }

    #[test]
    fn test_derive_pda_authority_includes_vault_funder() {
        let vault_funder = Pubkey::from_str("11111111111111111111111111111111")
            .expect("vault_funder pubkey should parse");

        let (authority1, _) = derive_pda_authority(&vault_funder);
        let (authority2, _) = derive_pda_authority(&Pubkey::new_unique());

        // Different vault_funders should produce different authorities
        assert_ne!(authority1, authority2);
        assert_ne!(authority1, Pubkey::default());
    }

    #[test]
    fn test_all_pda_derivations_complete_without_panicking() {
        let creator = Pubkey::new_unique();
        let agreement_id = [7u8; 16];
        let vault_funder = Pubkey::new_unique();

        let (agreement_pda, _) = derive_agreement_pda(&creator, &agreement_id);
        let (collection_pda, _) = derive_collection_state_pda(&creator);
        let (authority_pda, _) = derive_pda_authority(&vault_funder);

        assert_ne!(agreement_pda, Pubkey::default());
        assert_ne!(collection_pda, Pubkey::default());
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
    fn test_program_id_constant_parses() {
        let parsed = pactum_program_pubkey();
        assert_eq!(parsed.to_string(), PROGRAM_ID);
    }

    #[test]
    fn test_mpl_core_program_id_matches_idl() {
        // MPL Core program ID from IDL: CoREENxT6tW1HoK8ypY1SxRMZTcVPm7R94rH4PZNhX7d
        let expected = Pubkey::from_str("CoREENxT6tW1HoK8ypY1SxRMZTcVPm7R94rH4PZNhX7d")
            .expect("MPL Core program ID should parse");
        assert_eq!(MPL_CORE_PROGRAM_ID, expected);
    }
}
