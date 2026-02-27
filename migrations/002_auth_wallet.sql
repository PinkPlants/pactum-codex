-- Wallet auth method
CREATE TABLE auth_wallet (
    user_id    UUID NOT NULL REFERENCES user_accounts(id) ON DELETE CASCADE,
    pubkey     TEXT PRIMARY KEY,
    linked_at  BIGINT NOT NULL DEFAULT extract(epoch from now())
);

CREATE INDEX idx_auth_wallet_user ON auth_wallet(user_id);
