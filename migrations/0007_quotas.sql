-- Per-user storage quota caps.
-- A missing row means "use the built-in default cap"
-- (see QuotaService::DEFAULT_QUOTA, currently 1 GiB).
CREATE TABLE IF NOT EXISTS quotas (
    user_id   UUID PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    max_bytes BIGINT NOT NULL DEFAULT 1073741824
);

CREATE INDEX IF NOT EXISTS idx_quotas_user ON quotas(user_id);
