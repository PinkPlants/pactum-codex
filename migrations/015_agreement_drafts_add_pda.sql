-- Migration: Add agreement_pda column to agreement_drafts table
-- Fixes: Need to persist PDA for refund worker to JOIN against

ALTER TABLE agreement_drafts
    ADD COLUMN agreement_pda TEXT,
    ADD COLUMN agreement_id BYTEA;  -- 16-byte UUID stored as bytes for PDA derivation

-- Index for efficient lookups
CREATE INDEX idx_agreement_drafts_pda ON agreement_drafts(agreement_pda)
    WHERE agreement_pda IS NOT NULL;

COMMENT ON COLUMN agreement_drafts.agreement_pda IS 
    'On-chain agreement PDA. Computed from creator_pubkey + agreement_id, set on draft submission.';
COMMENT ON COLUMN agreement_drafts.agreement_id IS 
    'UUID used for PDA derivation. Stored to allow re-deriving PDA if needed.';
