use crate::error::AppError;
use crate::state::WsEvent;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

/// Notification event types from spec §12.4
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum NotificationEvent {
    AgreementCreated,
    Signed,
    Completed,
    Cancelled,
    Expired,
    RevokeVote,
    Revoked,
    DraftReadyToSubmit,
    InvitationExpired,
    InvitationReminder,
    PaymentConfirmed,
    RefundInitiated,
    RefundCompleted,
}

impl NotificationEvent {
    pub fn as_str(&self) -> &'static str {
        match self {
            NotificationEvent::AgreementCreated => "AgreementCreated",
            NotificationEvent::Signed => "Signed",
            NotificationEvent::Completed => "Completed",
            NotificationEvent::Cancelled => "Cancelled",
            NotificationEvent::Expired => "Expired",
            NotificationEvent::RevokeVote => "RevokeVote",
            NotificationEvent::Revoked => "Revoked",
            NotificationEvent::DraftReadyToSubmit => "DraftReadyToSubmit",
            NotificationEvent::InvitationExpired => "InvitationExpired",
            NotificationEvent::InvitationReminder => "InvitationReminder",
            NotificationEvent::PaymentConfirmed => "PaymentConfirmed",
            NotificationEvent::RefundInitiated => "RefundInitiated",
            NotificationEvent::RefundCompleted => "RefundCompleted",
        }
    }

    pub fn subject(&self) -> &'static str {
        match self {
            NotificationEvent::AgreementCreated => "You've been invited to sign an agreement",
            NotificationEvent::Signed => "Agreement partially signed",
            NotificationEvent::Completed => "Agreement fully signed — credential minted",
            NotificationEvent::Cancelled => "Agreement cancelled",
            NotificationEvent::Expired => "Agreement expired unsigned",
            NotificationEvent::RevokeVote => "A party voted to revoke the credential",
            NotificationEvent::Revoked => "Credential revoked by unanimous consent",
            NotificationEvent::DraftReadyToSubmit => {
                "All parties have joined — your agreement is ready to submit"
            }
            NotificationEvent::InvitationExpired => {
                "A party hasn't responded to your agreement invitation"
            }
            NotificationEvent::InvitationReminder => {
                "Reminder: you've been invited to sign an agreement"
            }
            NotificationEvent::PaymentConfirmed => {
                "Payment confirmed — your agreement is ready to submit"
            }
            NotificationEvent::RefundInitiated => "Your refund is being processed",
            NotificationEvent::RefundCompleted => "Your refund has been sent to your wallet",
        }
    }
}

/// Notification job from notification_queue table
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct NotificationJob {
    pub id: Uuid,
    pub event_type: String,
    pub agreement_pda: Option<String>,
    pub recipient_pubkey: String,
    pub scheduled_at: i64,
    pub status: String,
    pub attempts: i32,
}

/// Enqueue a notification for later dispatch
pub async fn enqueue_notification(
    db: &PgPool,
    event_type: &str,
    agreement_pda: Option<&str>,
    recipient_pubkey: &str,
) -> Result<(), AppError> {
    let now = chrono::Utc::now().timestamp();

    sqlx::query(
        "INSERT INTO notification_queue (event_type, agreement_pda, recipient_pubkey, scheduled_at, status, attempts)
         VALUES ($1, $2, $3, $4, 'pending', 0)"
    )
    .bind(event_type)
    .bind(agreement_pda)
    .bind(recipient_pubkey)
    .bind(now)
    .execute(db)
    .await
    .map_err(|_| AppError::InternalError)?;

    Ok(())
}

/// Fetch pending notification jobs
pub async fn fetch_pending_jobs(db: &PgPool, limit: i64) -> Result<Vec<NotificationJob>, AppError> {
    let jobs = sqlx::query_as::<_, NotificationJob>(
        r#"
        SELECT id, event_type, agreement_pda, recipient_pubkey, scheduled_at, status, attempts
        FROM notification_queue
        WHERE status = 'pending' AND scheduled_at <= $1
        ORDER BY scheduled_at ASC
        LIMIT $2
        "#,
    )
    .bind(chrono::Utc::now().timestamp())
    .bind(limit)
    .fetch_all(db)
    .await
    .map_err(|_| AppError::InternalError)?;

    Ok(jobs)
}

/// Mark notification as sent
pub async fn mark_sent(db: &PgPool, id: Uuid) -> Result<(), AppError> {
    sqlx::query("UPDATE notification_queue SET status = 'sent', sent_at = $1 WHERE id = $2")
        .bind(chrono::Utc::now().timestamp())
        .bind(id)
        .execute(db)
        .await
        .map_err(|_| AppError::InternalError)?;

    Ok(())
}

/// Increment notification attempt count
pub async fn increment_attempts(db: &PgPool, id: Uuid) -> Result<(), AppError> {
    sqlx::query("UPDATE notification_queue SET attempts = attempts + 1 WHERE id = $1")
        .bind(id)
        .execute(db)
        .await
        .map_err(|_| AppError::InternalError)?;

    Ok(())
}

/// Build WebSocket event from notification job
pub fn build_ws_event(job: &NotificationJob) -> WsEvent {
    match job.event_type.as_str() {
        "AgreementCreated" => WsEvent::AgreementCreated {
            agreement_pda: job.agreement_pda.clone().unwrap_or_default(),
        },
        "Signed" => WsEvent::AgreementSigned {
            agreement_pda: job.agreement_pda.clone().unwrap_or_default(),
        },
        "Completed" => WsEvent::AgreementCompleted {
            agreement_pda: job.agreement_pda.clone().unwrap_or_default(),
        },
        "Cancelled" => WsEvent::AgreementCancelled {
            agreement_pda: job.agreement_pda.clone().unwrap_or_default(),
        },
        "Expired" => WsEvent::AgreementExpired {
            agreement_pda: job.agreement_pda.clone().unwrap_or_default(),
        },
        "RevokeVote" => WsEvent::AgreementRevokeVote {
            agreement_pda: job.agreement_pda.clone().unwrap_or_default(),
        },
        "Revoked" => WsEvent::AgreementRevoked {
            agreement_pda: job.agreement_pda.clone().unwrap_or_default(),
        },
        "DraftReadyToSubmit" => WsEvent::DraftReady {
            draft_id: job.agreement_pda.clone().unwrap_or_default(),
        },
        "InvitationExpired" => WsEvent::DraftInvitationExpired {
            draft_id: job.agreement_pda.clone().unwrap_or_default(),
        },
        "PaymentConfirmed" => WsEvent::PaymentConfirmed {
            draft_id: job.agreement_pda.clone().unwrap_or_default(),
        },
        "RefundCompleted" => WsEvent::RefundCompleted {
            draft_id: job.agreement_pda.clone().unwrap_or_default(),
        },
        _ => WsEvent::GenericNotification {
            message: format!("Event: {}", job.event_type),
        },
    }
}

/// Email dispatch using Resend API
pub async fn send_email(
    resend_api_key: &str,
    from_email: &str,
    recipient_email: &str,
    event: &NotificationEvent,
    agreement_pda: Option<&str>,
) -> Result<(), AppError> {
    let subject = event.subject();
    let html_body = render_email_html(event, agreement_pda);
    let text_body = render_email_text(event, agreement_pda);

    let client = reqwest::Client::new();
    let response = client
        .post("https://api.resend.com/emails")
        .header("Authorization", format!("Bearer {resend_api_key}"))
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({
            "from": from_email,
            "to": recipient_email,
            "subject": subject,
            "html": html_body,
            "text": text_body,
        }))
        .send()
        .await
        .map_err(|e| {
            tracing::error!("Failed to send email via Resend: {e}");
            AppError::InternalError
        })?;

    if response.status().is_success() {
        tracing::info!("Email sent to {recipient_email} - Subject: {subject}");
        Ok(())
    } else {
        let status = response.status();
        let error_text = response.text().await.unwrap_or_default();
        tracing::error!("Resend API error: {status} - {error_text}");
        Err(AppError::InternalError)
    }
}

/// Render HTML email body for event
fn render_email_html(event: &NotificationEvent, agreement_pda: Option<&str>) -> String {
    let pda_display = agreement_pda.unwrap_or("N/A");
    let subject = event.subject();
    let main_content = match event {
        NotificationEvent::AgreementCreated => format!(
            r#"<p>You've been invited to sign an agreement.</p>
            <p>Agreement ID: <code>{}</code></p>
            <p><a href="https://app.pactum.app/agreement/{}" style="background:#4F46E5;color:white;padding:12px 24px;text-decoration:none;border-radius:6px;">View Agreement</a></p>"#,
            pda_display, pda_display
        ),
        NotificationEvent::Signed => format!(
            r#"<p>A party has signed the agreement.</p>
            <p>Agreement ID: <code>{}</code></p>"#,
            pda_display
        ),
        NotificationEvent::Completed => format!(
            r#"<p>Congratulations! The agreement has been fully signed and the credential has been minted.</p>
            <p>Agreement ID: <code>{}</code></p>
            <p><a href="https://app.pactum.app/agreement/{}" style="background:#10B981;color:white;padding:12px 24px;text-decoration:none;border-radius:6px;">View Credential</a></p>"#,
            pda_display, pda_display
        ),
        NotificationEvent::Cancelled => format!(
            r#"<p>The agreement has been cancelled.</p>
            <p>Agreement ID: <code>{}</code></p>"#,
            pda_display
        ),
        NotificationEvent::Expired => format!(
            r#"<p>The agreement has expired unsigned.</p>
            <p>Agreement ID: <code>{}</code></p>"#,
            pda_display
        ),
        NotificationEvent::RevokeVote => format!(
            r#"<p>A party has voted to revoke the credential.</p>
            <p>Agreement ID: <code>{}</code></p>"#,
            pda_display
        ),
        NotificationEvent::Revoked => format!(
            r#"<p>The credential has been revoked by unanimous consent.</p>
            <p>Agreement ID: <code>{}</code></p>"#,
            pda_display
        ),
        NotificationEvent::DraftReadyToSubmit => format!(
            r#"<p>All parties have joined your agreement draft and it's ready to submit.</p>
            <p><a href="https://app.pactum.app/drafts" style="background:#4F46E5;color:white;padding:12px 24px;text-decoration:none;border-radius:6px;">Submit Agreement</a></p>"#
        ),
        NotificationEvent::InvitationExpired => format!(
            r#"<p>A party hasn't responded to your agreement invitation within the time limit.</p>
            <p>You can resend the invitation or discard the draft.</p>"#
        ),
        NotificationEvent::InvitationReminder => format!(
            r#"<p>This is a reminder that you've been invited to sign an agreement.</p>
            <p>Agreement ID: <code>{}</code></p>"#,
            pda_display
        ),
        NotificationEvent::PaymentConfirmed => format!(
            r#"<p>Your payment has been confirmed and your agreement is ready to submit.</p>
            <p><a href="https://app.pactum.app/drafts" style="background:#4F46E5;color:white;padding:12px 24px;text-decoration:none;border-radius:6px;">Submit Agreement</a></p>"#
        ),
        NotificationEvent::RefundInitiated => format!(
            r#"<p>Your refund is being processed.</p>
            <p>This may take a few minutes to complete.</p>"#
        ),
        NotificationEvent::RefundCompleted => format!(
            r#"<p>Your refund has been sent to your wallet.</p>
            <p>The funds should appear in your account shortly.</p>"#
        ),
    };

    format!(
        r#"<!DOCTYPE html>
<html>
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>{}</title>
</head>
<body style="font-family:system-ui,-apple-system,sans-serif;line-height:1.5;color:#1f2937;max-width:600px;margin:0 auto;padding:24px;">
    <div style="background:#f9fafb;padding:24px;border-radius:8px;margin-bottom:24px;">
        <h1 style="color:#111827;margin:0 0 16px 0;font-size:24px;">Pactum</h1>
        <h2 style="color:#4b5563;margin:0;font-size:18px;font-weight:normal;">{}</h2>
    </div>
    <div style="background:white;padding:24px;border-radius:8px;border:1px solid #e5e7eb;">
        {}
    </div>
    <div style="margin-top:24px;padding-top:24px;border-top:1px solid #e5e7eb;color:#6b7280;font-size:14px;">
        <p>This is an automated message from Pactum. Please do not reply to this email.</p>
    </div>
</body>
</html>"#,
        subject, subject, main_content
    )
}

/// Render plain text email body for event
fn render_email_text(event: &NotificationEvent, agreement_pda: Option<&str>) -> String {
    let pda_display = agreement_pda.unwrap_or("N/A");
    match event {
        NotificationEvent::AgreementCreated => format!(
            "You've been invited to sign an agreement.\n\nAgreement ID: {}\n\nView at: https://app.pactum.app/agreement/{}",
            pda_display, pda_display
        ),
        NotificationEvent::Signed => format!(
            "A party has signed the agreement.\n\nAgreement ID: {}",
            pda_display
        ),
        NotificationEvent::Completed => format!(
            "Congratulations! The agreement has been fully signed and the credential has been minted.\n\nAgreement ID: {}\n\nView at: https://app.pactum.app/agreement/{}",
            pda_display, pda_display
        ),
        NotificationEvent::Cancelled => format!(
            "The agreement has been cancelled.\n\nAgreement ID: {}",
            pda_display
        ),
        NotificationEvent::Expired => format!(
            "The agreement has expired unsigned.\n\nAgreement ID: {}",
            pda_display
        ),
        NotificationEvent::RevokeVote => format!(
            "A party has voted to revoke the credential.\n\nAgreement ID: {}",
            pda_display
        ),
        NotificationEvent::Revoked => format!(
            "The credential has been revoked by unanimous consent.\n\nAgreement ID: {}",
            pda_display
        ),
        NotificationEvent::DraftReadyToSubmit => 
            "All parties have joined your agreement draft and it's ready to submit.\n\nSubmit at: https://app.pactum.app/drafts".to_string(),
        NotificationEvent::InvitationExpired => 
            "A party hasn't responded to your agreement invitation within the time limit.\n\nYou can resend the invitation or discard the draft.".to_string(),
        NotificationEvent::InvitationReminder => format!(
            "This is a reminder that you've been invited to sign an agreement.\n\nAgreement ID: {}",
            pda_display
        ),
        NotificationEvent::PaymentConfirmed => 
            "Your payment has been confirmed and your agreement is ready to submit.\n\nSubmit at: https://app.pactum.app/drafts".to_string(),
        NotificationEvent::RefundInitiated => 
            "Your refund is being processed.\n\nThis may take a few minutes to complete.".to_string(),
        NotificationEvent::RefundCompleted => 
            "Your refund has been sent to your wallet.\n\nThe funds should appear in your account shortly.".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_notification_event_enum_has_all_13_types() {
        // Verify all 13 event types exist
        let events = vec![
            NotificationEvent::AgreementCreated,
            NotificationEvent::Signed,
            NotificationEvent::Completed,
            NotificationEvent::Cancelled,
            NotificationEvent::Expired,
            NotificationEvent::RevokeVote,
            NotificationEvent::Revoked,
            NotificationEvent::DraftReadyToSubmit,
            NotificationEvent::InvitationExpired,
            NotificationEvent::InvitationReminder,
            NotificationEvent::PaymentConfirmed,
            NotificationEvent::RefundInitiated,
            NotificationEvent::RefundCompleted,
        ];

        assert_eq!(events.len(), 13, "Should have exactly 13 event types");
    }

    #[test]
    fn test_notification_event_as_str() {
        assert_eq!(
            NotificationEvent::AgreementCreated.as_str(),
            "AgreementCreated"
        );
        assert_eq!(NotificationEvent::Completed.as_str(), "Completed");
        assert_eq!(
            NotificationEvent::RefundCompleted.as_str(),
            "RefundCompleted"
        );
    }

    #[test]
    fn test_notification_event_subject() {
        assert_eq!(
            NotificationEvent::AgreementCreated.subject(),
            "You've been invited to sign an agreement"
        );
        assert_eq!(
            NotificationEvent::Completed.subject(),
            "Agreement fully signed — credential minted"
        );
    }

    #[test]
    fn test_build_ws_event_agreement_created() {
        let job = NotificationJob {
            id: Uuid::new_v4(),
            event_type: "AgreementCreated".to_string(),
            agreement_pda: Some("test_pda".to_string()),
            recipient_pubkey: "test_pubkey".to_string(),
            scheduled_at: 0,
            status: "pending".to_string(),
            attempts: 0,
        };

        let event = build_ws_event(&job);

        match event {
            WsEvent::AgreementCreated { agreement_pda } => {
                assert_eq!(agreement_pda, "test_pda");
            }
            _ => panic!("Expected AgreementCreated event"),
        }
    }

    #[test]
    fn test_build_ws_event_unknown_type_falls_back_to_generic() {
        let job = NotificationJob {
            id: Uuid::new_v4(),
            event_type: "UnknownEvent".to_string(),
            agreement_pda: None,
            recipient_pubkey: "test_pubkey".to_string(),
            scheduled_at: 0,
            status: "pending".to_string(),
            attempts: 0,
        };

        let event = build_ws_event(&job);

        match event {
            WsEvent::GenericNotification { message } => {
                assert!(message.contains("UnknownEvent"));
            }
            _ => panic!("Expected GenericNotification event"),
        }
    }

    #[test]
    fn test_build_ws_event_revoke_vote() {
        let job = NotificationJob {
            id: Uuid::new_v4(),
            event_type: "RevokeVote".to_string(),
            agreement_pda: Some("vote_pda".to_string()),
            recipient_pubkey: "test_pubkey".to_string(),
            scheduled_at: 0,
            status: "pending".to_string(),
            attempts: 0,
        };

        let event = build_ws_event(&job);

        match event {
            WsEvent::AgreementRevokeVote { agreement_pda } => {
                assert_eq!(agreement_pda, "vote_pda");
            }
            _ => panic!("Expected AgreementRevokeVote event"),
        }
    }

    #[test]
    fn test_build_ws_event_invitation_expired() {
        let job = NotificationJob {
            id: Uuid::new_v4(),
            event_type: "InvitationExpired".to_string(),
            agreement_pda: Some("draft_123".to_string()),
            recipient_pubkey: "test_pubkey".to_string(),
            scheduled_at: 0,
            status: "pending".to_string(),
            attempts: 0,
        };

        let event = build_ws_event(&job);

        match event {
            WsEvent::DraftInvitationExpired { draft_id } => {
                assert_eq!(draft_id, "draft_123");
            }
            _ => panic!("Expected DraftInvitationExpired event"),
        }
    }
}
