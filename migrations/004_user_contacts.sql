CREATE TABLE user_contacts (
    user_id          UUID PRIMARY KEY REFERENCES user_accounts(id) ON DELETE CASCADE,
    -- Encrypted fields (AES-256-GCM ciphertext + nonce)
    email_enc        BYTEA,
    email_nonce      BYTEA,
    email_index      BYTEA,   -- HMAC blind index for exact-match lookup
    phone_enc        BYTEA,
    phone_nonce      BYTEA,
    push_token_enc   BYTEA,
    push_token_nonce BYTEA,
    updated_at       BIGINT NOT NULL DEFAULT extract(epoch from now())
);

-- Allows "does this email already exist?" lookup without decryption
CREATE INDEX idx_user_contacts_email_index ON user_contacts(email_index);
