-- Team quota caps (R-21 / Q-6): per-team byte ceilings, mirroring the
-- per-user `quotas` table. A missing row means "use the built-in default
-- cap". Kept as its own table so the existing `quotas` PK (user_id) is
-- untouched.

CREATE TABLE IF NOT EXISTS team_quotas (
    team_id   UUID PRIMARY KEY REFERENCES teams(id) ON DELETE CASCADE,
    max_bytes BIGINT NOT NULL DEFAULT 1073741824
);

