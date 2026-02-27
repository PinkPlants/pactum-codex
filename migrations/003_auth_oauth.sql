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

CREATE INDEX idx_auth_oauth_user  ON auth_oauth(user_id);
