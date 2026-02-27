-- migrations/012_payment_tx_sig_unique.sql
CREATE UNIQUE INDEX idx_payments_tx_sig
    ON agreement_payments(token_tx_signature)
    WHERE token_tx_signature IS NOT NULL;
