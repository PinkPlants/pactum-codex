CREATE TABLE notification_queue (
    id               UUID    PRIMARY KEY DEFAULT gen_random_uuid(),
    event_type       TEXT    NOT NULL,   -- 'AgreementCreated', 'Signed', etc.
    agreement_pda    TEXT    NOT NULL,
    recipient_pubkey TEXT    NOT NULL,
    status           TEXT    NOT NULL DEFAULT 'pending',  -- pending | sent | failed
    attempts         INT     NOT NULL DEFAULT 0,
    created_at       BIGINT  NOT NULL DEFAULT extract(epoch from now()),
    scheduled_at     BIGINT  NOT NULL DEFAULT extract(epoch from now())
);

CREATE INDEX idx_notification_queue_pending
    ON notification_queue(status, scheduled_at)
    WHERE status = 'pending';
