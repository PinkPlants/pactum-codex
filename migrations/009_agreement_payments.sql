CREATE TABLE agreement_payments (
    id               UUID    PRIMARY KEY DEFAULT gen_random_uuid(),
    draft_id         UUID    NOT NULL REFERENCES agreement_drafts(id) ON DELETE CASCADE,
    user_id          UUID    NOT NULL REFERENCES user_accounts(id),
    method           TEXT    NOT NULL,
    -- method: 'usdc' | 'usdt' | 'pyusd'
    status           TEXT    NOT NULL DEFAULT 'pending',
    -- status: pending | confirmed | refund_pending | refunded | failed

    -- USD amount charged
    usd_amount_cents INT     NOT NULL,  -- always 199 ($1.99)

    -- Stablecoin fields (all supported tokens have 6 decimals — amount always 1_990_000)
    token_reference_pubkey TEXT UNIQUE,  -- Solana Pay reference for tx identification
    token_mint             TEXT,         -- verified mint from StablecoinRegistry
    token_amount           BIGINT,       -- always 1_990_000 (1.99 × 10^6)
    token_tx_signature     TEXT,         -- confirmed Solana tx signature
    token_source_ata       TEXT,         -- platform treasury ATA used for this payment;
                                         -- stored at payment initiation for refund ATA validation (H-5)

    -- Refund fields (populated on cancel/expire)
    refund_amount          BIGINT,       -- token base units refunded; 0 if no refund
    refund_usd_cents       INT,          -- USD equivalent kept for accounting; = usd_amount_cents - nonrefundable
    refund_tx_signature    TEXT,         -- SPL transfer tx signature for the refund
    refund_initiated_at    BIGINT,
    refund_completed_at    BIGINT,

    created_at   BIGINT  NOT NULL DEFAULT extract(epoch from now()),
    confirmed_at BIGINT
);

CREATE INDEX idx_agreement_payments_draft  ON agreement_payments(draft_id);
CREATE INDEX idx_agreement_payments_user   ON agreement_payments(user_id);
CREATE INDEX idx_agreement_payments_token  ON agreement_payments(token_reference_pubkey)
    WHERE token_reference_pubkey IS NOT NULL;
CREATE INDEX idx_agreement_payments_refund ON agreement_payments(status)
    WHERE status = 'refund_pending';  -- efficient scan for pending refund jobs
