use crate::error::AppError;
use crate::handlers::ws::send_to_user;
use crate::middleware::wallet_guard::AuthUserWithWallet;
use crate::services::notification::enqueue_notification;
use crate::state::{AppState, WsEvent};
use axum::{
    extract::{Path, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    Json,
};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use std::sync::OnceLock;
use uuid::Uuid;

const RATE_LIMIT_MAX_REQUESTS: usize = 5;
const RATE_LIMIT_WINDOW_SECS: i64 = 60;

static INVITE_RATE_LIMITER: OnceLock<DashMap<String, Vec<i64>>> = OnceLock::new();

#[derive(Debug, Serialize)]
pub struct GetInviteResponse {
    pub agreement_title: String,
    pub creator_display: String,
    pub expires_at: i64,
    pub has_account: bool,
    pub has_wallet: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DraftPartyEntry {
    pubkey: Option<String>,
    invite_id: Option<Uuid>,
}

#[derive(Debug, FromRow)]
struct InviteViewRow {
    id: Uuid,
    draft_id: Uuid,
    status: String,
    expires_at: i64,
    agreement_title: Option<String>,
    creator_display: Option<String>,
    has_account: bool,
    has_wallet: bool,
}

#[derive(Debug, FromRow)]
struct InviteAcceptRow {
    id: Uuid,
    draft_id: Uuid,
    status: String,
    expires_at: i64,
}

#[derive(Debug, FromRow)]
struct DraftForAcceptRow {
    party_slots: serde_json::Value,
    creator_pubkey: String,
}

pub async fn get_invite(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(token): Path<String>,
) -> Result<Json<GetInviteResponse>, AppError> {
    enforce_get_invite_rate_limit(&headers, current_unix_timestamp())?;

    let row = sqlx::query_as::<_, InviteViewRow>(
        "SELECT
            pi.id,
            pi.draft_id,
            pi.status,
            pi.expires_at,
            ad.draft_payload->>'title' AS agreement_title,
            ua.display_name AS creator_display,
            EXISTS(
                SELECT 1 FROM user_contacts uc
                WHERE uc.email_index = pi.invited_email_index
            ) AS has_account,
            EXISTS(
                SELECT 1
                FROM user_contacts uc
                JOIN auth_wallet aw ON aw.user_id = uc.user_id
                WHERE uc.email_index = pi.invited_email_index
            ) AS has_wallet
         FROM party_invitations pi
         JOIN agreement_drafts ad ON ad.id = pi.draft_id
         LEFT JOIN auth_wallet creator_aw ON creator_aw.pubkey = ad.creator_pubkey
         LEFT JOIN user_accounts ua ON ua.id = creator_aw.user_id
         WHERE pi.token = $1",
    )
    .bind(token)
    .fetch_optional(&state.db)
    .await
    .map_err(|_| AppError::InternalError)?
    .ok_or(AppError::NotFound)?;

    validate_pending_invitation(&row.status, row.expires_at, current_unix_timestamp())?;

    Ok(Json(GetInviteResponse {
        agreement_title: row.agreement_title.unwrap_or_default(),
        creator_display: row
            .creator_display
            .unwrap_or_else(|| "Unknown creator".to_string()),
        expires_at: row.expires_at,
        has_account: row.has_account,
        has_wallet: row.has_wallet,
    }))
}

pub async fn accept_invite(
    State(state): State<AppState>,
    auth: AuthUserWithWallet,
    Path(token): Path<String>,
) -> Result<StatusCode, AppError> {
    let now = current_unix_timestamp();
    let mut tx = state
        .db
        .begin()
        .await
        .map_err(|_| AppError::InternalError)?;

    let invite = sqlx::query_as::<_, InviteAcceptRow>(
        "SELECT id, draft_id, status, expires_at
         FROM party_invitations
         WHERE token = $1
         FOR UPDATE",
    )
    .bind(&token)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|_| AppError::InternalError)?
    .ok_or(AppError::NotFound)?;

    validate_pending_invitation(&invite.status, invite.expires_at, now)?;

    let draft = sqlx::query_as::<_, DraftForAcceptRow>(
        "SELECT party_slots, creator_pubkey
         FROM agreement_drafts
         WHERE id = $1
         FOR UPDATE",
    )
    .bind(invite.draft_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|_| AppError::InternalError)?
    .ok_or(AppError::NotFound)?;

    let mut party_slots: Vec<DraftPartyEntry> =
        serde_json::from_value(draft.party_slots).map_err(|_| AppError::InternalError)?;

    resolve_party_slot_pubkey(&mut party_slots, invite.id, auth.pubkey.as_str())?;
    let all_resolved = all_pubkeys_resolved(&party_slots);
    let party_slots_json =
        serde_json::to_value(&party_slots).map_err(|_| AppError::InternalError)?;

    sqlx::query("UPDATE party_invitations SET status = 'accepted' WHERE id = $1")
        .bind(invite.id)
        .execute(&mut *tx)
        .await
        .map_err(|_| AppError::InternalError)?;

    if all_resolved {
        sqlx::query(
            "UPDATE agreement_drafts
             SET party_slots = $1, status = 'ready_to_submit', ready_at = $2
             WHERE id = $3",
        )
        .bind(party_slots_json)
        .bind(now)
        .bind(invite.draft_id)
        .execute(&mut *tx)
        .await
        .map_err(|_| AppError::InternalError)?;
    } else {
        sqlx::query("UPDATE agreement_drafts SET party_slots = $1 WHERE id = $2")
            .bind(party_slots_json)
            .bind(invite.draft_id)
            .execute(&mut *tx)
            .await
            .map_err(|_| AppError::InternalError)?;
    }

    tx.commit().await.map_err(|_| AppError::InternalError)?;

    if all_resolved {
        let draft_id_string = invite.draft_id.to_string();
        let creator_user_id =
            sqlx::query_scalar::<_, Uuid>("SELECT user_id FROM auth_wallet WHERE pubkey = $1")
                .bind(&draft.creator_pubkey)
                .fetch_optional(&state.db)
                .await
                .map_err(|_| AppError::InternalError)?;

        if let Some(user_id) = creator_user_id {
            let delivered = send_to_user(
                &state,
                user_id,
                WsEvent::DraftReady {
                    draft_id: draft_id_string.clone(),
                },
            );

            if !delivered {
                enqueue_notification(
                    &state.db,
                    "DraftReadyToSubmit",
                    Some(&draft_id_string),
                    &draft.creator_pubkey,
                )
                .await?;
            }
        }
    }

    Ok(StatusCode::NO_CONTENT)
}

fn validate_pending_invitation(status: &str, expires_at: i64, now: i64) -> Result<(), AppError> {
    if status != "pending" || expires_at < now {
        return Err(AppError::NotFound);
    }
    Ok(())
}

fn resolve_party_slot_pubkey(
    slots: &mut [DraftPartyEntry],
    invite_id: Uuid,
    pubkey: &str,
) -> Result<(), AppError> {
    for slot in slots {
        if slot.invite_id == Some(invite_id) {
            slot.pubkey = Some(pubkey.to_string());
            slot.invite_id = None;
            return Ok(());
        }
    }
    Err(AppError::NotFound)
}

fn all_pubkeys_resolved(slots: &[DraftPartyEntry]) -> bool {
    slots.iter().all(|slot| slot.pubkey.is_some())
}

fn enforce_get_invite_rate_limit(headers: &HeaderMap, now: i64) -> Result<(), AppError> {
    let limiter = INVITE_RATE_LIMITER.get_or_init(DashMap::new);
    let ip = extract_request_ip(headers);

    let mut entry = limiter.entry(ip).or_default();
    entry.retain(|timestamp| now - *timestamp < RATE_LIMIT_WINDOW_SECS);

    if entry.len() >= RATE_LIMIT_MAX_REQUESTS {
        return Err(AppError::RateLimited);
    }

    entry.push(now);
    Ok(())
}

fn extract_request_ip(headers: &HeaderMap) -> String {
    parse_ip_header(headers.get("x-forwarded-for"))
        .or_else(|| parse_ip_header(headers.get("x-real-ip")))
        .unwrap_or_else(|| "unknown".to_string())
}

fn parse_ip_header(value: Option<&HeaderValue>) -> Option<String> {
    value
        .and_then(|header| header.to_str().ok())
        .and_then(|raw| raw.split(',').next())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn current_unix_timestamp() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now();
    now.duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    #[test]
    fn rate_limiter_rejects_more_than_five_per_minute() {
        let mut headers = HeaderMap::new();
        let ip = format!("198.51.100.{}", Uuid::new_v4().as_u128() % 250 + 1);
        headers.insert(
            "x-forwarded-for",
            HeaderValue::from_str(&ip).expect("valid test IP"),
        );

        let now = 1_700_000_000_i64;
        for offset in 0..RATE_LIMIT_MAX_REQUESTS {
            let result =
                enforce_get_invite_rate_limit(&headers, now + i64::try_from(offset).unwrap_or(0));
            assert!(result.is_ok());
        }

        let limited = enforce_get_invite_rate_limit(&headers, now + 10);
        assert!(matches!(limited, Err(AppError::RateLimited)));
    }

    #[test]
    fn party_slots_update_sets_pubkey_for_matching_invite_id() {
        let invite_id = Uuid::new_v4();
        let mut slots = vec![
            DraftPartyEntry {
                pubkey: Some("Party111111111111111111111111111111111".to_string()),
                invite_id: None,
            },
            DraftPartyEntry {
                pubkey: None,
                invite_id: Some(invite_id),
            },
        ];

        resolve_party_slot_pubkey(
            &mut slots,
            invite_id,
            "Wallet222222222222222222222222222222222",
        )
        .expect("slot should resolve");

        assert_eq!(
            slots[1].pubkey.as_deref(),
            Some("Wallet222222222222222222222222222222222")
        );
        assert_eq!(slots[1].invite_id, None);
    }

    #[test]
    fn all_resolved_detection_returns_true_when_pubkeys_present() {
        let slots = vec![
            DraftPartyEntry {
                pubkey: Some("Party111111111111111111111111111111111".to_string()),
                invite_id: None,
            },
            DraftPartyEntry {
                pubkey: Some("Party222222222222222222222222222222222".to_string()),
                invite_id: None,
            },
        ];

        assert!(all_pubkeys_resolved(&slots));
    }
}
