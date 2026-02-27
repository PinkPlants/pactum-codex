CREATE TABLE siws_nonces (
    nonce      TEXT    PRIMARY KEY,
    created_at BIGINT  NOT NULL DEFAULT extract(epoch from now())
);
-- No explicit TTL column needed — keeper cleans up expired rows every 60s:
-- DELETE FROM siws_nonces WHERE created_at < extract(epoch from now()) - 300;
