ALTER TABLE agreement_drafts
    ADD COLUMN paid              BOOLEAN NOT NULL DEFAULT false,
    ADD COLUMN paid_at           BIGINT,
    ADD COLUMN payment_id        UUID REFERENCES agreement_payments(id),
    ADD COLUMN storage_uri       TEXT,      -- set after document uploaded to Arweave/IPFS
    ADD COLUMN storage_uploaded  BOOLEAN NOT NULL DEFAULT false;
    -- storage_uploaded = false → nothing spent yet → full refund on cancel/expire
    -- storage_uploaded = true  → Arweave/IPFS fee spent → partial refund ($1.89); $0.10 kept

CREATE TABLE user_agreement_counts (
    user_id           UUID PRIMARY KEY REFERENCES user_accounts(id) ON DELETE CASCADE,
    total_submitted   INT  NOT NULL DEFAULT 0,   -- incremented when create_agreement confirmed on-chain
    free_used         INT  NOT NULL DEFAULT 0    -- incremented for each free agreement consumed
);
