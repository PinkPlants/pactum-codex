-- Consolidated user and authentication tables
-- Merges: 001_user_accounts, 002_auth_wallet, 003_auth_oauth, 004_user_contacts, 011_siws_nonces, 013_refresh_tokens

-- Core identity table; one row per user
CREATE TABLE user_accounts (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    display_name TEXT,            -- optional; user-provided; no trust value on-chain
    created_at   BIGINT NOT NULL DEFAULT extract(epoch from now())
);

-- Wallet auth method
CREATE TABLE auth_wallet (
    user_id    UUID NOT NULL REFERENCES user_accounts(id) ON DELETE CASCADE,
    pubkey     TEXT PRIMARY KEY,
    linked_at  BIGINT NOT NULL DEFAULT extract(epoch from now())
);

CREATE INDEX idx_auth_wallet_user ON auth_wallet(user_id);

-- OAuth auth method — supports any provider via the `provider` column
-- Supported v0.1: 'google' | 'microsoft'
-- Planned: 'apple' (future version)
CREATE TABLE auth_oauth (
    user_id     UUID NOT NULL REFERENCES user_accounts(id) ON DELETE CASCADE,
    provider    TEXT NOT NULL,         -- 'google' | 'microsoft' | 'apple'
    provider_id TEXT NOT NULL,         -- provider's stable user ID (sub / oid / sub)
    linked_at   BIGINT NOT NULL DEFAULT extract(epoch from now()),
    PRIMARY KEY (provider, provider_id)
);

CREATE INDEX idx_auth_oauth_user ON auth_oauth(user_id);

-- Encrypted user contact information
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

-- SIWS (Sign In With Solana) nonces
CREATE TABLE siws_nonces (
    nonce      TEXT    PRIMARY KEY,
    created_at BIGINT  NOT NULL DEFAULT extract(epoch from now())
);
-- No explicit TTL column needed — keeper cleans up expired rows every 60s:
-- DELETE FROM siws_nonces WHERE created_at < extract(epoch from now()) - 300;

-- JWT refresh tokens for session management
CREATE TABLE refresh_tokens (
    token_hash  TEXT    PRIMARY KEY,   -- SHA-256(token) — plaintext never stored
    user_id     UUID    NOT NULL REFERENCES user_accounts(id) ON DELETE CASCADE,
    created_at  BIGINT  NOT NULL DEFAULT extract(epoch from now()),
    expires_at  BIGINT  NOT NULL       -- created_at + 604800 (7 days)
);

CREATE INDEX idx_refresh_tokens_user ON refresh_tokens(user_id);
