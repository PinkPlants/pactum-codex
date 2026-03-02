use crate::error::AppError;
use crate::middleware::auth::AuthUser;
use crate::middleware::wallet_guard::AuthUserWithWallet;
use crate::services::crypto::{encrypt, hmac_index};
use crate::services::solana::{
    build_create_agreement_tx, build_sign_agreement_tx, derive_agreement_pda,
};
use crate::solana_types::{AgreementStateWire, CreateAgreementArgs, StorageBackend};
use crate::state::AppState;
use aes_gcm::aead::{rand_core::RngCore, OsRng};
use axum::{
    extract::{Path, Query, State},
    Json,
};
use borsh::BorshDeserialize;
use serde::{Deserialize, Serialize};
use solana_sdk::pubkey::Pubkey;
use sqlx::FromRow;
use std::str::FromStr;
use uuid::Uuid;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateAgreementRequest {
    pub title: String,
    pub parties: Vec<PartyInput>,
    pub expires_in_secs: u64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignAgreementRequest {
    pub metadata_uri: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum PartyInput {
    Pubkey(PartyPubkeyInput),
    Email(PartyEmailInput),
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PartyPubkeyInput {
    pub pubkey: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PartyEmailInput {
    pub email: String,
}

#[derive(Debug, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum CreateAgreementResponse {
    Submitted {
        transaction: String,
        agreement_pda: String,
    },
    AwaitingPartyWallets {
        draft_id: Uuid,
        pending_invitations: Vec<PendingInvitation>,
    },
}

#[derive(Debug, Serialize)]
pub struct SignAgreementResponse {
    pub transaction: String,
    pub suggest_email: bool,
    pub suggest_email_reason: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct GetAgreementResponse {
    pub creator: String,
    pub agreement_id: String,
    pub title: String,
    pub storage_uri: String,
    pub storage_backend: String,
    pub status: String,
    pub created_at: i64,
    pub expires_at: i64,
    pub parties: Vec<String>,
    pub signed_by: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ListAgreementsQuery {
    pub status: Option<String>,
    pub role: Option<String>,
    pub page: Option<u32>,
    pub limit: Option<u32>,
}

#[derive(Debug, Serialize)]
pub struct ListAgreementsResponse {
    pub page: u32,
    pub limit: u32,
    pub items: Vec<ListAgreementItem>,
}

#[derive(Debug, Serialize)]
pub struct ListAgreementItem {
    pub agreement_pda: String,
    pub creator_pubkey: String,
    pub party_pubkey: String,
    pub status: String,
    pub signed_at: Option<i64>,
    pub created_at: i64,
    pub expires_at: i64,
    pub title: String,
}

#[derive(Debug, Serialize)]
pub struct PendingInvitation {
    pub email_hint: String,
    pub invited_at: i64,
}

#[derive(Debug, Serialize)]
#[serde(deny_unknown_fields)]
struct DraftPayload {
    title: String,
    expires_in_secs: u64,
    parties: Vec<DraftPartyEntry>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(deny_unknown_fields)]
struct DraftPartyEntry {
    pubkey: Option<String>,
    invite_id: Option<Uuid>,
}

pub async fn create_agreement(
    State(state): State<AppState>,
    auth: AuthUserWithWallet,
    Json(req): Json<CreateAgreementRequest>,
) -> Result<Json<CreateAgreementResponse>, AppError> {
    validate_invite_window(state.config.invite_expiry_seconds, req.expires_in_secs)?;

    let encryption_key = decode_hex_key_32(&state.config.encryption_key)?;
    let encryption_index_key = decode_hex_key_32(&state.config.encryption_index_key)?;

    let free_used = sqlx::query_scalar::<_, i32>(
        "SELECT free_used FROM user_agreement_counts WHERE user_id = $1",
    )
    .bind(auth.user_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|_| AppError::InternalError)?
    .unwrap_or(0);
    let payment_requirement_is_free = free_used < state.config.platform_fee_free_tier as i32;

    let mut all_parties_resolved = true;
    let mut party_slots: Vec<DraftPartyEntry> = Vec::with_capacity(req.parties.len());
    let mut invitation_rows: Vec<InvitationInsertRow> = Vec::new();
    let invited_at = current_unix_timestamp();
    let expires_at = invited_at
        + i64::try_from(state.config.invite_expiry_seconds).map_err(|_| AppError::InternalError)?;

    for party in &req.parties {
        match party {
            PartyInput::Pubkey(party_pubkey) => {
                party_slots.push(DraftPartyEntry {
                    pubkey: Some(party_pubkey.pubkey.clone()),
                    invite_id: None,
                });
            }
            PartyInput::Email(party_email) => {
                let email_index = hmac_index(&party_email.email, &encryption_index_key);
                let resolved_pubkey = sqlx::query_scalar::<_, String>(
                    "SELECT aw.pubkey
                     FROM user_contacts uc
                     JOIN auth_wallet aw ON aw.user_id = uc.user_id
                     WHERE uc.email_index = $1",
                )
                .bind(&email_index)
                .fetch_optional(&state.db)
                .await
                .map_err(|_| AppError::InternalError)?;

                if let Some(pubkey) = resolved_pubkey {
                    party_slots.push(DraftPartyEntry {
                        pubkey: Some(pubkey),
                        invite_id: None,
                    });
                } else {
                    all_parties_resolved = false;
                    let invite_id = Uuid::new_v4();
                    let mut token_bytes = [0u8; 32];
                    OsRng.fill_bytes(&mut token_bytes);
                    let token = hex::encode(token_bytes);
                    let (invited_email_enc, invited_email_nonce) =
                        encrypt(&party_email.email, &encryption_key)?;

                    party_slots.push(DraftPartyEntry {
                        pubkey: None,
                        invite_id: Some(invite_id),
                    });

                    invitation_rows.push(InvitationInsertRow {
                        id: invite_id,
                        email_index,
                        email_enc: invited_email_enc,
                        email_nonce: invited_email_nonce.to_vec(),
                        token,
                        email_hint: email_hint(&party_email.email),
                        invited_at,
                        expires_at,
                    });
                }
            }
        }
    }

    if all_parties_resolved && payment_requirement_is_free {
        let creator_pubkey = Pubkey::from_str(&auth.pubkey).map_err(|_| AppError::InternalError)?;
        let agreement_id = Uuid::new_v4();
        let agreement_id_bytes: [u8; 16] = *agreement_id.as_bytes();
        let (agreement_pda, _) = derive_agreement_pda(&creator_pubkey, &agreement_id_bytes);
        let args = CreateAgreementArgs {
            agreement_id: agreement_id.to_string(),
            title: req.title.clone(),
            content_hash: "placeholder_content_hash".to_string(),
            storage_uri: "ipfs://placeholder".to_string(),
            storage_backend: StorageBackend::Ipfs,
            parties: party_slots
                .iter()
                .filter_map(|party| party.pubkey.clone())
                .collect(),
            vault_deposit: 0,
            expires_in_secs: req.expires_in_secs,
        };
        let transaction = build_create_agreement_tx(
            state.solana.as_ref(),
            &args,
            &creator_pubkey,
            state.vault_keypair.as_ref(),
            state.config.as_ref(),
        )
        .await?;

        return Ok(Json(CreateAgreementResponse::Submitted {
            transaction,
            agreement_pda: agreement_pda.to_string(),
        }));
    }

    let draft_payload = DraftPayload {
        title: req.title,
        expires_in_secs: req.expires_in_secs,
        parties: party_slots.clone(),
    };
    let draft_payload_json =
        serde_json::to_value(&draft_payload).map_err(|_| AppError::InternalError)?;
    let party_slots_json =
        serde_json::to_value(&party_slots).map_err(|_| AppError::InternalError)?;

    let draft_id: Uuid = sqlx::query_scalar(
        "INSERT INTO agreement_drafts (creator_pubkey, draft_payload, party_slots, status)
         VALUES ($1, $2, $3, $4)
         RETURNING id",
    )
    .bind(&auth.pubkey)
    .bind(draft_payload_json)
    .bind(party_slots_json)
    .bind("awaiting_party_wallets")
    .fetch_one(&state.db)
    .await
    .map_err(|_| AppError::InternalError)?;

    for invite in &invitation_rows {
        sqlx::query(
            "INSERT INTO party_invitations (
                id,
                draft_id,
                invited_email_index,
                invited_email_enc,
                invited_email_nonce,
                token,
                expires_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(invite.id)
        .bind(draft_id)
        .bind(&invite.email_index)
        .bind(&invite.email_enc)
        .bind(&invite.email_nonce)
        .bind(&invite.token)
        .bind(invite.expires_at)
        .execute(&state.db)
        .await
        .map_err(|_| AppError::InternalError)?;
    }

    let pending_invitations = invitation_rows
        .into_iter()
        .map(|invite| PendingInvitation {
            email_hint: invite.email_hint,
            invited_at: invite.invited_at,
        })
        .collect();

    Ok(Json(CreateAgreementResponse::AwaitingPartyWallets {
        draft_id,
        pending_invitations,
    }))
}

pub async fn sign_agreement(
    State(state): State<AppState>,
    auth: AuthUserWithWallet,
    Path(pda): Path<String>,
    Json(req): Json<SignAgreementRequest>,
) -> Result<Json<SignAgreementResponse>, AppError> {
    let pda_pubkey = Pubkey::from_str(&pda).map_err(|_| AppError::InternalError)?;
    let signer_pubkey = Pubkey::from_str(&auth.pubkey).map_err(|_| AppError::InternalError)?;

    let account_data = state
        .solana
        .get_account_data(&pda_pubkey)
        .map_err(|_| AppError::InternalError)?;
    if account_data.len() < 8 {
        return Err(AppError::InternalError);
    }

    let agreement = AgreementStateWire::try_from_slice(&account_data[8..])
        .map_err(|_| AppError::InternalError)?;

    if !contains_pubkey_bytes(&agreement.parties, &signer_pubkey) {
        return Err(AppError::Unauthorized);
    }

    let signer_already_signed = contains_pubkey_bytes(&agreement.signed_by, &signer_pubkey);
    if is_final_signature(
        agreement.parties.len(),
        agreement.signed_by.len(),
        signer_already_signed,
    ) && req.metadata_uri.is_none()
    {
        return Err(AppError::InternalError);
    }

    let creator_pubkey = Pubkey::new_from_array(agreement.creator);
    let transaction = build_sign_agreement_tx(
        state.solana.as_ref(),
        &creator_pubkey,
        &agreement.agreement_id,
        &signer_pubkey,
        req.metadata_uri,
    )
    .await?;

    let email_enc = sqlx::query_scalar::<_, Option<Vec<u8>>>(
        "SELECT email_enc FROM user_contacts WHERE user_id = $1",
    )
    .bind(auth.user_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|_| AppError::InternalError)?;

    let suggest_email = !matches!(email_enc, Some(Some(_)));
    let suggest_email_reason = suggest_email.then(|| {
        "Add your email to be notified when all parties sign and your credential is ready."
            .to_string()
    });

    Ok(Json(SignAgreementResponse {
        transaction,
        suggest_email,
        suggest_email_reason,
    }))
}

pub async fn get_agreement(
    State(state): State<AppState>,
    Path(pda): Path<String>,
) -> Result<Json<GetAgreementResponse>, AppError> {
    let pda_pubkey = Pubkey::from_str(&pda).map_err(|_| AppError::InternalError)?;

    let data = state
        .solana
        .get_account_data(&pda_pubkey)
        .map_err(|_| AppError::NotFound)?;

    if data.len() < 8 {
        return Err(AppError::NotFound);
    }

    let agreement =
        AgreementStateWire::try_from_slice(&data[8..]).map_err(|_| AppError::NotFound)?;
    let agreement_id = Uuid::from_slice(&agreement.agreement_id)
        .map(|id| id.to_string())
        .unwrap_or_else(|_| hex::encode(agreement.agreement_id));

    let response = GetAgreementResponse {
        creator: Pubkey::new_from_array(agreement.creator).to_string(),
        agreement_id,
        title: agreement.title,
        storage_uri: agreement.storage_uri,
        storage_backend: storage_backend_to_string(agreement.storage_backend),
        status: agreement_status_to_string(agreement.status),
        created_at: agreement.created_at,
        expires_at: agreement.expires_at,
        parties: pubkey_bytes_to_base58_vec(&agreement.parties),
        signed_by: pubkey_bytes_to_base58_vec(&agreement.signed_by),
    };

    Ok(Json(response))
}

pub async fn list_agreements(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(q): Query<ListAgreementsQuery>,
) -> Result<Json<ListAgreementsResponse>, AppError> {
    let (page, limit, sql_limit, sql_offset) = sanitize_pagination(q.page, q.limit);

    let Some(pubkey) = auth.pubkey else {
        return Ok(Json(ListAgreementsResponse {
            page,
            limit,
            items: Vec::new(),
        }));
    };

    let role = parse_role(q.role.as_deref());
    let status_filter = q.status.and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    });

    let rows = match role {
        AgreementListRole::Any => {
            sqlx::query_as::<_, AgreementPartyRow>(
                "SELECT agreement_pda, creator_pubkey, party_pubkey, status, signed_at, created_at, expires_at, title
                 FROM agreement_parties
                 WHERE (party_pubkey = $1 OR creator_pubkey = $1)
                   AND ($2::text IS NULL OR status = $2)
                 ORDER BY created_at DESC
                 LIMIT $3 OFFSET $4",
            )
            .bind(&pubkey)
            .bind(status_filter.as_deref())
            .bind(sql_limit)
            .bind(sql_offset)
            .fetch_all(&state.db)
            .await
            .map_err(|_| AppError::InternalError)?
        }
        AgreementListRole::Creator => {
            sqlx::query_as::<_, AgreementPartyRow>(
                "SELECT agreement_pda, creator_pubkey, party_pubkey, status, signed_at, created_at, expires_at, title
                 FROM agreement_parties
                 WHERE creator_pubkey = $1
                   AND ($2::text IS NULL OR status = $2)
                 ORDER BY created_at DESC
                 LIMIT $3 OFFSET $4",
            )
            .bind(&pubkey)
            .bind(status_filter.as_deref())
            .bind(sql_limit)
            .bind(sql_offset)
            .fetch_all(&state.db)
            .await
            .map_err(|_| AppError::InternalError)?
        }
        AgreementListRole::Party => {
            sqlx::query_as::<_, AgreementPartyRow>(
                "SELECT agreement_pda, creator_pubkey, party_pubkey, status, signed_at, created_at, expires_at, title
                 FROM agreement_parties
                 WHERE party_pubkey = $1
                   AND ($2::text IS NULL OR status = $2)
                 ORDER BY created_at DESC
                 LIMIT $3 OFFSET $4",
            )
            .bind(&pubkey)
            .bind(status_filter.as_deref())
            .bind(sql_limit)
            .bind(sql_offset)
            .fetch_all(&state.db)
            .await
            .map_err(|_| AppError::InternalError)?
        }
    };

    let items = rows
        .into_iter()
        .map(|row| ListAgreementItem {
            agreement_pda: row.agreement_pda,
            creator_pubkey: row.creator_pubkey,
            party_pubkey: row.party_pubkey,
            status: row.status,
            signed_at: row.signed_at,
            created_at: row.created_at,
            expires_at: row.expires_at,
            title: row.title,
        })
        .collect();

    Ok(Json(ListAgreementsResponse { page, limit, items }))
}

#[derive(Debug)]
struct InvitationInsertRow {
    id: Uuid,
    email_index: Vec<u8>,
    email_enc: Vec<u8>,
    email_nonce: Vec<u8>,
    token: String,
    email_hint: String,
    invited_at: i64,
    expires_at: i64,
}

#[derive(Debug, FromRow)]
struct AgreementPartyRow {
    agreement_pda: String,
    creator_pubkey: String,
    party_pubkey: String,
    status: String,
    signed_at: Option<i64>,
    created_at: i64,
    expires_at: i64,
    title: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AgreementListRole {
    Creator,
    Party,
    Any,
}

fn validate_invite_window(
    invite_expiry_seconds: u64,
    expires_in_secs: u64,
) -> Result<(), AppError> {
    if invite_expiry_seconds >= expires_in_secs {
        return Err(AppError::InviteWindowExceedsSigningWindow);
    }
    Ok(())
}

fn decode_hex_key_32(key_hex: &str) -> Result<[u8; 32], AppError> {
    let key_bytes = hex::decode(key_hex).map_err(|_| AppError::InternalError)?;
    key_bytes.try_into().map_err(|_| AppError::InternalError)
}

fn email_hint(email: &str) -> String {
    let (local, domain) = match email.split_once('@') {
        Some(parts) => parts,
        None => return "***".to_string(),
    };

    let first = local.chars().next().unwrap_or('*');
    format!("{}***@{}", first, domain)
}

fn current_unix_timestamp() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now();
    now.duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn contains_pubkey_bytes(pubkeys: &[[u8; 32]], pubkey: &Pubkey) -> bool {
    let bytes = pubkey.to_bytes();
    pubkeys.iter().any(|candidate| candidate == &bytes)
}

fn is_final_signature(
    parties_len: usize,
    signed_by_len: usize,
    signer_already_signed: bool,
) -> bool {
    parties_len == signed_by_len + 1 && !signer_already_signed
}

fn parse_role(role: Option<&str>) -> AgreementListRole {
    match role {
        Some(value) if value.eq_ignore_ascii_case("creator") => AgreementListRole::Creator,
        Some(value) if value.eq_ignore_ascii_case("party") => AgreementListRole::Party,
        _ => AgreementListRole::Any,
    }
}

fn sanitize_pagination(page: Option<u32>, limit: Option<u32>) -> (u32, u32, i64, i64) {
    let page = page.unwrap_or(1).max(1);
    let limit = limit.unwrap_or(20).clamp(1, 100);
    let offset = page.saturating_sub(1).saturating_mul(limit) as i64;
    (page, limit, i64::from(limit), offset)
}

fn pubkey_bytes_to_base58_vec(keys: &[[u8; 32]]) -> Vec<String> {
    keys.iter()
        .map(|bytes| Pubkey::new_from_array(*bytes).to_string())
        .collect()
}

fn agreement_status_to_string(status: crate::solana_types::AgreementStatus) -> String {
    match status {
        crate::solana_types::AgreementStatus::Draft => "draft",
        crate::solana_types::AgreementStatus::PendingSignatures => "pending_signatures",
        crate::solana_types::AgreementStatus::Completed => "completed",
        crate::solana_types::AgreementStatus::Cancelled => "cancelled",
        crate::solana_types::AgreementStatus::Expired => "expired",
        crate::solana_types::AgreementStatus::Revoked => "revoked",
    }
    .to_string()
}

fn storage_backend_to_string(storage_backend: StorageBackend) -> String {
    match storage_backend {
        StorageBackend::Ipfs => "ipfs",
        StorageBackend::Arweave => "arweave",
    }
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_invite_window_validation_triggers_error() {
        let result = validate_invite_window(7200, 7200);
        assert!(matches!(
            result,
            Err(AppError::InviteWindowExceedsSigningWindow)
        ));
    }

    #[test]
    fn test_email_hint_masking() {
        assert_eq!(email_hint("bob@example.com"), "b***@example.com");
    }

    #[test]
    fn test_is_final_signature_true_when_one_signature_remaining() {
        assert!(is_final_signature(3, 2, false));
    }

    #[test]
    fn test_is_final_signature_false_when_signer_already_signed() {
        assert!(!is_final_signature(3, 2, true));
    }

    #[test]
    fn test_sanitize_pagination_defaults_and_caps_limit() {
        let (page, limit, sql_limit, sql_offset) = sanitize_pagination(None, Some(999));
        assert_eq!(page, 1);
        assert_eq!(limit, 100);
        assert_eq!(sql_limit, 100);
        assert_eq!(sql_offset, 0);
    }
}
