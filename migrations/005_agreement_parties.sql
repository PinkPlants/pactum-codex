-- One row per (party, agreement) pair — the primary reason SQL exists
CREATE TABLE agreement_parties (
    party_pubkey   TEXT    NOT NULL,
    agreement_pda  TEXT    NOT NULL,
    creator_pubkey TEXT    NOT NULL,
    status         TEXT    NOT NULL DEFAULT 'PendingSignatures',
    signed_at      BIGINT,           -- NULL until this party has signed
    created_at     BIGINT  NOT NULL,
    expires_at     BIGINT  NOT NULL,
    title          TEXT    NOT NULL,
    PRIMARY KEY (party_pubkey, agreement_pda)
);

CREATE INDEX idx_agreement_parties_pubkey  ON agreement_parties(party_pubkey);
CREATE INDEX idx_agreement_parties_status  ON agreement_parties(party_pubkey, status);
CREATE INDEX idx_agreement_parties_pda     ON agreement_parties(agreement_pda);
