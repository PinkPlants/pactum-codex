CREATE TABLE party_invitations (
    id                  UUID    PRIMARY KEY DEFAULT gen_random_uuid(),
    draft_id            UUID    NOT NULL REFERENCES agreement_drafts(id) ON DELETE CASCADE,
    invited_email_index BYTEA   NOT NULL,  -- HMAC blind index for lookup
    invited_email_enc   BYTEA   NOT NULL,  -- AES-256-GCM ciphertext
    invited_email_nonce BYTEA   NOT NULL,
    -- 32-byte CSPRNG hex-encoded to 64 characters — 256 bits of entropy (M-6 fix)
    token               TEXT    NOT NULL UNIQUE,
    status              TEXT    NOT NULL DEFAULT 'pending',
    -- status: pending | accepted | expired
    reminder_sent_at    BIGINT,
    reminder_count      INT     NOT NULL DEFAULT 0,
    created_at          BIGINT  NOT NULL DEFAULT extract(epoch from now()),
    expires_at          BIGINT  NOT NULL
);

CREATE INDEX idx_party_invitations_token        ON party_invitations(token);
CREATE INDEX idx_party_invitations_draft        ON party_invitations(draft_id);
CREATE INDEX idx_party_invitations_email_index  ON party_invitations(invited_email_index);
CREATE INDEX idx_party_invitations_pending      ON party_invitations(status, expires_at)
    WHERE status = 'pending';
