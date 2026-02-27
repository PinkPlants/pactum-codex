CREATE TABLE refresh_tokens (
    token_hash  TEXT    PRIMARY KEY,   -- SHA-256(token) — plaintext never stored
    user_id     UUID    NOT NULL REFERENCES user_accounts(id) ON DELETE CASCADE,
    created_at  BIGINT  NOT NULL DEFAULT extract(epoch from now()),
    expires_at  BIGINT  NOT NULL       -- created_at + 604800 (7 days)
);
CREATE INDEX idx_refresh_tokens_user ON refresh_tokens(user_id);
