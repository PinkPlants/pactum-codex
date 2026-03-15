-- Sweep configuration table for treasury sweep tracking
-- Stores the timestamp of the last successful treasury sweep

CREATE TABLE sweep_config (
    id            INTEGER PRIMARY KEY DEFAULT 1,
    last_sweep_at BIGINT NOT NULL
);

-- Insert default row for singleton pattern
-- Using epoch 0 means "never swept" so first sweep will always run
INSERT INTO sweep_config (id, last_sweep_at) VALUES (1, 0);
