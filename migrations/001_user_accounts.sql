-- Core identity table; one row per user
CREATE TABLE user_accounts (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    display_name TEXT,            -- optional; user-provided; no trust value on-chain
    created_at   BIGINT NOT NULL DEFAULT extract(epoch from now())
);
