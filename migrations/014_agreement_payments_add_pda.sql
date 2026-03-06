-- Migration: Add agreement_pda column to agreement_payments table
-- Fixes: refund_worker query failure due to missing column

ALTER TABLE agreement_payments
    ADD COLUMN agreement_pda TEXT;

-- Index for efficient lookups by agreement_pda (partial index for non-null values)
CREATE INDEX idx_agreement_payments_pda ON agreement_payments(agreement_pda)
    WHERE agreement_pda IS NOT NULL;

COMMENT ON COLUMN agreement_payments.agreement_pda IS 
    'On-chain agreement PDA. Set when draft is submitted and create_agreement transaction is built.';
