-- Agreement drafts and payments tables
-- Merges: 007_agreement_drafts, 009_agreement_payments, 010_user_agreement_counts,
--         012_payment_tx_sig_unique, 014_agreement_payments_add_pda, 015_agreement_drafts_add_pda

-- Agreement drafts table (pre-chain agreement preparation)
-- Note: payment_id FK is added after agreement_payments table is created (circular dependency)
CREATE TABLE agreement_drafts (
    id               UUID    PRIMARY KEY DEFAULT gen_random_uuid(),
    creator_pubkey   TEXT    NOT NULL,
    draft_payload    JSONB   NOT NULL,
    party_slots      JSONB   NOT NULL,
    status           TEXT    NOT NULL DEFAULT 'awaiting_party_wallets',
    created_at       BIGINT  NOT NULL DEFAULT extract(epoch from now()),
    ready_at         BIGINT,
    submitted_at     BIGINT,
    paid             BOOLEAN NOT NULL DEFAULT false,
    paid_at          BIGINT,
    storage_uri      TEXT,
    storage_uploaded BOOLEAN NOT NULL DEFAULT false,
    agreement_pda    TEXT,
    agreement_id     BYTEA
);

CREATE INDEX idx_agreement_drafts_creator ON agreement_drafts(creator_pubkey);
CREATE INDEX idx_agreement_drafts_status  ON agreement_drafts(status);
CREATE INDEX idx_agreement_drafts_pda ON agreement_drafts(agreement_pda)
    WHERE agreement_pda IS NOT NULL;

-- Agreement payments table
CREATE TABLE agreement_payments (
    id               UUID    PRIMARY KEY DEFAULT gen_random_uuid(),
    draft_id         UUID    NOT NULL REFERENCES agreement_drafts(id) ON DELETE CASCADE,
    user_id          UUID    NOT NULL REFERENCES user_accounts(id),
    method           TEXT    NOT NULL,
    status           TEXT    NOT NULL DEFAULT 'pending',
    usd_amount_cents INT     NOT NULL,
    token_reference_pubkey TEXT UNIQUE,
    token_mint             TEXT,
    token_amount           BIGINT,
    token_tx_signature     TEXT,
    token_source_ata       TEXT,
    refund_amount          BIGINT,
    refund_usd_cents       INT,
    refund_tx_signature    TEXT,
    refund_initiated_at    BIGINT,
    refund_completed_at    BIGINT,
    created_at   BIGINT  NOT NULL DEFAULT extract(epoch from now()),
    confirmed_at BIGINT,
    agreement_pda TEXT
);

CREATE INDEX idx_agreement_payments_draft  ON agreement_payments(draft_id);
CREATE INDEX idx_agreement_payments_user   ON agreement_payments(user_id);
CREATE INDEX idx_agreement_payments_token  ON agreement_payments(token_reference_pubkey)
    WHERE token_reference_pubkey IS NOT NULL;
CREATE INDEX idx_agreement_payments_refund ON agreement_payments(status)
    WHERE status = 'refund_pending';
CREATE INDEX idx_agreement_payments_pda ON agreement_payments(agreement_pda)
    WHERE agreement_pda IS NOT NULL;

-- Unique partial index
CREATE UNIQUE INDEX idx_payments_tx_sig ON agreement_payments(token_tx_signature)
    WHERE token_tx_signature IS NOT NULL;

-- Add payment_id FK to agreement_drafts (circular dependency resolution)
ALTER TABLE agreement_drafts
    ADD COLUMN payment_id UUID REFERENCES agreement_payments(id);

-- User agreement counts
CREATE TABLE user_agreement_counts (
    user_id           UUID PRIMARY KEY REFERENCES user_accounts(id) ON DELETE CASCADE,
    total_submitted   INT  NOT NULL DEFAULT 0,
    free_used         INT  NOT NULL DEFAULT 0
);
