CREATE TABLE agreement_drafts (
    id               UUID    PRIMARY KEY DEFAULT gen_random_uuid(),
    creator_pubkey   TEXT    NOT NULL,
    -- Stores title, parties, expires_in_secs only — NO document, NO storage_uri at this stage
    -- Written exclusively by backend handler — never raw user JSON.
    -- Deserialised via DraftPayload struct with #[serde(deny_unknown_fields)] (M-2).
    draft_payload    JSONB   NOT NULL,
    -- Tracks resolution status of each party slot
    -- e.g. [{"pubkey": "ABC..."}, {"pubkey": null, "invite_id": "uuid"}]
    -- email_hint is NOT stored here — only in party_invitations (PII isolation, L-4)
    party_slots      JSONB   NOT NULL,
    status           TEXT    NOT NULL DEFAULT 'awaiting_party_wallets',
    -- status: awaiting_party_wallets | ready_to_submit | submitted | discarded
    created_at       BIGINT  NOT NULL DEFAULT extract(epoch from now()),
    ready_at         BIGINT,    -- set when all pubkeys resolved
    submitted_at     BIGINT     -- set when create_agreement confirmed on-chain
    -- NOTE: no document_enc or document_key fields — document is never stored here.
    -- Upload to Arweave/IPFS is deferred until POST /draft/{id}/submit,
    -- ensuring zero storage fees are incurred for discarded drafts.
);

CREATE INDEX idx_agreement_drafts_creator ON agreement_drafts(creator_pubkey);
CREATE INDEX idx_agreement_drafts_status  ON agreement_drafts(status);
