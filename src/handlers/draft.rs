use crate::error::AppError;
use crate::middleware::auth::AuthUser;
use crate::middleware::wallet_guard::AuthUserWithWallet;
use crate::services::solana::{build_create_agreement_tx, derive_agreement_pda};
use crate::solana_types::{CreateAgreementArgs, StorageBackend};
use crate::state::AppState;
use aes_gcm::aead::{rand_core::RngCore, OsRng};
use axum::{
    extract::{Path, State},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;
use uuid::Uuid;

#[derive(Debug, Serialize)]
pub struct GetDraftResponse {
    pub id: Uuid,
    pub status: String,
    pub party_slots: Value,
    pub pending_invitations: Vec<PendingInvitation>,
}

#[derive(Debug, Serialize)]
pub struct PendingInvitation {
    pub id: Uuid,
    pub status: String,
    pub expires_at: i64,
}

#[derive(Debug, Serialize)]
pub struct ReinviteResponse {
    pub invitation_id: Uuid,
    pub expires_at: i64,
}

#[derive(Debug, Serialize)]
pub struct SubmitDraftResponse {
    pub transaction: String,
    pub agreement_pda: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReinviteRequest {
    pub invitation_id: Uuid,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SubmitDraftRequest {
    pub content_hash: String,
    pub storage_uri: String,
    pub storage_backend: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DraftPayload {
    title: String,
    expires_in_secs: u64,
    parties: Vec<DraftPartyEntry>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DraftPartyEntry {
    pubkey: Option<String>,
    invite_id: Option<Uuid>,
}

#[derive(Debug)]
struct DraftRow {
    creator_pubkey: String,
    draft_payload: Value,
    status: String,
    paid: bool,
}

pub async fn get_draft(
    State(state): State<AppState>,
    _auth: AuthUser,
    Path(draft_id): Path<Uuid>,
) -> Result<Json<GetDraftResponse>, AppError> {
    let draft_row = sqlx::query_as::<_, (Uuid, String, Value)>(
        "SELECT id, status, party_slots FROM agreement_drafts WHERE id = $1",
    )
    .bind(draft_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|_| AppError::InternalError)?
    .ok_or(AppError::NotFound)?;

    let pending_rows = sqlx::query_as::<_, (Uuid, String, i64)>(
        "SELECT id, status, expires_at
         FROM party_invitations
         WHERE draft_id = $1 AND status = 'pending'
         ORDER BY created_at ASC",
    )
    .bind(draft_id)
    .fetch_all(&state.db)
    .await
    .map_err(|_| AppError::InternalError)?;

    let pending_invitations = pending_rows
        .into_iter()
        .map(|(id, status, expires_at)| PendingInvitation {
            id,
            status,
            expires_at,
        })
        .collect();

    Ok(Json(GetDraftResponse {
        id: draft_row.0,
        status: draft_row.1,
        party_slots: draft_row.2,
        pending_invitations,
    }))
}

pub async fn delete_draft(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(draft_id): Path<Uuid>,
) -> Result<(), AppError> {
    let draft_creator = sqlx::query_scalar::<_, String>(
        "SELECT creator_pubkey FROM agreement_drafts WHERE id = $1",
    )
    .bind(draft_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|_| AppError::InternalError)?
    .ok_or(AppError::NotFound)?;

    ensure_creator_from_user_id(&state.db, auth.user_id, &draft_creator).await?;

    sqlx::query("UPDATE agreement_drafts SET status = 'discarded' WHERE id = $1")
        .bind(draft_id)
        .execute(&state.db)
        .await
        .map_err(|_| AppError::InternalError)?;

    Ok(())
}

pub async fn reinvite_draft(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(draft_id): Path<Uuid>,
    Json(req): Json<ReinviteRequest>,
) -> Result<Json<ReinviteResponse>, AppError> {
    let draft_creator = sqlx::query_scalar::<_, String>(
        "SELECT creator_pubkey FROM agreement_drafts WHERE id = $1",
    )
    .bind(draft_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|_| AppError::InternalError)?
    .ok_or(AppError::NotFound)?;

    ensure_creator_from_user_id(&state.db, auth.user_id, &draft_creator).await?;

    let expired_invitation = sqlx::query_as::<_, (Vec<u8>, Vec<u8>, Vec<u8>)>(
        "SELECT invited_email_index, invited_email_enc, invited_email_nonce
         FROM party_invitations
         WHERE id = $1 AND draft_id = $2 AND status = 'expired'",
    )
    .bind(req.invitation_id)
    .bind(draft_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|_| AppError::InternalError)?
    .ok_or(AppError::NotFound)?;

    let mut token_bytes = [0u8; 32];
    OsRng.fill_bytes(&mut token_bytes);
    let token = hex::encode(token_bytes);

    let expires_at = current_unix_timestamp()
        + i64::try_from(state.config.invite_expiry_seconds).map_err(|_| AppError::InternalError)?;

    let invitation_id: Uuid = sqlx::query_scalar(
        "INSERT INTO party_invitations (
            draft_id,
            invited_email_index,
            invited_email_enc,
            invited_email_nonce,
            token,
            expires_at
        )
        VALUES ($1, $2, $3, $4, $5, $6)
        RETURNING id",
    )
    .bind(draft_id)
    .bind(expired_invitation.0)
    .bind(expired_invitation.1)
    .bind(expired_invitation.2)
    .bind(token)
    .bind(expires_at)
    .fetch_one(&state.db)
    .await
    .map_err(|_| AppError::InternalError)?;

    Ok(Json(ReinviteResponse {
        invitation_id,
        expires_at,
    }))
}

pub async fn submit_draft(
    State(state): State<AppState>,
    auth: AuthUserWithWallet,
    Path(draft_id): Path<Uuid>,
    Json(req): Json<SubmitDraftRequest>,
) -> Result<Json<SubmitDraftResponse>, AppError> {
    validate_content_hash(&req.content_hash)?;
    let storage_backend = parse_storage_backend(&req.storage_backend)?;

    let draft = get_draft_row(&state, draft_id).await?;
    ensure_creator_only(Some(&auth.pubkey), &draft.creator_pubkey)?;

    let has_email = has_contact_email(&state, auth.user_id).await?;
    evaluate_submit_gates(draft.paid, &draft.status, has_email, draft_id)?;

    sqlx::query(
        "UPDATE agreement_drafts
         SET storage_uri = $1, storage_uploaded = true
         WHERE id = $2",
    )
    .bind(&req.storage_uri)
    .bind(draft_id)
    .execute(&state.db)
    .await
    .map_err(|_| AppError::InternalError)?;

    let payload: DraftPayload =
        serde_json::from_value(draft.draft_payload).map_err(|_| AppError::InternalError)?;
    let parties: Vec<String> = payload
        .parties
        .iter()
        .filter_map(|party| {
            let _ = party.invite_id;
            party.pubkey.clone()
        })
        .collect();

    let creator_pubkey = Pubkey::from_str(&auth.pubkey).map_err(|_| AppError::InternalError)?;
    let agreement_id = Uuid::new_v4();

    let args = CreateAgreementArgs {
        agreement_id: agreement_id.to_string(),
        title: payload.title,
        content_hash: req.content_hash,
        storage_uri: req.storage_uri,
        storage_backend,
        parties,
        vault_deposit: 0,
        expires_in_secs: payload.expires_in_secs,
    };

    let transaction = build_create_agreement_tx(
        state.solana.as_ref(),
        &args,
        &creator_pubkey,
        state.vault_keypair.as_ref(),
        state.config.as_ref(),
    )
    .await?;

    let agreement_id_bytes: [u8; 16] = *agreement_id.as_bytes();
    let (agreement_pda, _) = derive_agreement_pda(&creator_pubkey, &agreement_id_bytes);

    Ok(Json(SubmitDraftResponse {
        transaction,
        agreement_pda: agreement_pda.to_string(),
    }))
}

async fn get_draft_row(state: &AppState, draft_id: Uuid) -> Result<DraftRow, AppError> {
    let row = sqlx::query_as::<_, (String, Value, String, bool)>(
        "SELECT creator_pubkey, draft_payload, status, paid
         FROM agreement_drafts
         WHERE id = $1",
    )
    .bind(draft_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|_| AppError::InternalError)?
    .ok_or(AppError::NotFound)?;

    Ok(DraftRow {
        creator_pubkey: row.0,
        draft_payload: row.1,
        status: row.2,
        paid: row.3,
    })
}

async fn has_contact_email(state: &AppState, user_id: Uuid) -> Result<bool, AppError> {
    let has_email = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(
            SELECT 1 FROM user_contacts
            WHERE user_id = $1 AND email_enc IS NOT NULL
        )",
    )
    .bind(user_id)
    .fetch_one(&state.db)
    .await
    .map_err(|_| AppError::InternalError)?;

    Ok(has_email)
}

async fn ensure_creator_from_user_id(
    db: &sqlx::PgPool,
    user_id: Uuid,
    draft_creator_pubkey: &str,
) -> Result<(), AppError> {
    let auth_pubkey =
        sqlx::query_scalar::<_, String>("SELECT pubkey FROM auth_wallet WHERE user_id = $1")
            .bind(user_id)
            .fetch_optional(db)
            .await
            .map_err(|_| AppError::InternalError)?;

    ensure_creator_only(auth_pubkey.as_deref(), draft_creator_pubkey)
}

fn ensure_creator_only(
    auth_pubkey: Option<&str>,
    draft_creator_pubkey: &str,
) -> Result<(), AppError> {
    match auth_pubkey {
        Some(pubkey) if pubkey == draft_creator_pubkey => Ok(()),
        _ => Err(AppError::Unauthorized),
    }
}

fn evaluate_submit_gates(
    paid: bool,
    status: &str,
    has_email: bool,
    draft_id: Uuid,
) -> Result<(), AppError> {
    if !paid {
        return Err(AppError::PaymentRequired {
            draft_id: draft_id.to_string(),
            initiate_url: format!("/payment/initiate/{draft_id}"),
        });
    }

    if status != "ready_to_submit" {
        return Err(AppError::DraftNotReady);
    }

    if !has_email {
        return Err(AppError::EmailRequired {
            message: "Add an email address to receive agreement notifications.".to_string(),
            add_email_url: "/user/contacts".to_string(),
        });
    }

    Ok(())
}

fn parse_storage_backend(raw: &str) -> Result<StorageBackend, AppError> {
    match raw.to_ascii_lowercase().as_str() {
        "ipfs" => Ok(StorageBackend::Ipfs),
        "arweave" => Ok(StorageBackend::Arweave),
        _ => Err(AppError::UploadFailed),
    }
}

fn validate_content_hash(content_hash: &str) -> Result<(), AppError> {
    if content_hash.len() != 64 {
        return Err(AppError::InvalidHash);
    }

    let is_hex = content_hash
        .as_bytes()
        .iter()
        .all(|byte| byte.is_ascii_hexdigit());

    if !is_hex {
        return Err(AppError::InvalidHash);
    }

    Ok(())
}

fn current_unix_timestamp() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now();
    now.duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_submit_gate_payment_required_is_first_gate() {
        let draft_id = Uuid::new_v4();
        let result = evaluate_submit_gates(false, "awaiting_party_wallets", false, draft_id);

        assert!(matches!(result, Err(AppError::PaymentRequired { .. })));
    }

    #[test]
    fn test_submit_gate_draft_not_ready_is_second_gate() {
        let result = evaluate_submit_gates(true, "awaiting_party_wallets", true, Uuid::new_v4());

        assert!(matches!(result, Err(AppError::DraftNotReady)));
    }

    #[test]
    fn test_submit_gate_email_required_is_third_gate() {
        let result = evaluate_submit_gates(true, "ready_to_submit", false, Uuid::new_v4());

        assert!(matches!(result, Err(AppError::EmailRequired { .. })));
    }

    #[test]
    fn test_submit_gate_success_when_all_requirements_met() {
        let result = evaluate_submit_gates(true, "ready_to_submit", true, Uuid::new_v4());

        assert!(result.is_ok());
    }

    #[test]
    fn test_creator_only_authorization_passes_for_matching_pubkey() {
        let result = ensure_creator_only(Some("creator_pubkey"), "creator_pubkey");

        assert!(result.is_ok());
    }

    #[test]
    fn test_creator_only_authorization_rejects_non_creator() {
        let result = ensure_creator_only(Some("other_pubkey"), "creator_pubkey");

        assert!(matches!(result, Err(AppError::Unauthorized)));
    }
}
