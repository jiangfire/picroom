-- Audit log + jobs + API tokens
-- Phase 3.

CREATE TABLE IF NOT EXISTS api_tokens (
    id              UUID PRIMARY KEY,
    user_id         UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name            VARCHAR(255) NOT NULL,
    hash            CHAR(64) NOT NULL,
    last_four       CHAR(4) NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_used_at    TIMESTAMPTZ,
    revoked_at      TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_api_tokens_user ON api_tokens(user_id);

CREATE TABLE IF NOT EXISTS audit_events (
    id              UUID PRIMARY KEY,
    timestamp       TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    actor_id        UUID REFERENCES users(id) ON DELETE SET NULL,
    actor_label     VARCHAR(255),
    action          VARCHAR(64) NOT NULL,
    target_type     VARCHAR(64) NOT NULL,
    target_id       VARCHAR(255),
    ip              INET,
    user_agent      VARCHAR(512),
    metadata        JSONB NOT NULL DEFAULT '{}'::jsonb
);

CREATE INDEX IF NOT EXISTS idx_audit_timestamp ON audit_events(timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_audit_actor ON audit_events(actor_id);
CREATE INDEX IF NOT EXISTS idx_audit_action ON audit_events(action);
CREATE INDEX IF NOT EXISTS idx_audit_target ON audit_events(target_type, target_id);

CREATE TABLE IF NOT EXISTS jobs (
    id              UUID PRIMARY KEY,
    image_id        UUID NOT NULL REFERENCES images(id) ON DELETE CASCADE,
    kind            VARCHAR(32) NOT NULL
                       CHECK (kind IN ('encode_avif', 'encode_webp', 'thumbnail', 'watermark', 'strip_exif')),
    payload         JSONB NOT NULL DEFAULT '{}'::jsonb,
    status          VARCHAR(32) NOT NULL DEFAULT 'pending'
                       CHECK (status IN ('pending', 'running', 'succeeded', 'failed', 'dead')),
    attempts        INTEGER NOT NULL DEFAULT 0,
    last_error      TEXT,
    enqueued_at     TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    started_at      TIMESTAMPTZ,
    finished_at     TIMESTAMPTZ
);

-- Partial index for the worker's hot path: find pending jobs fast.
CREATE INDEX IF NOT EXISTS idx_jobs_pending ON jobs(enqueued_at)
    WHERE status = 'pending';

CREATE INDEX IF NOT EXISTS idx_jobs_image ON jobs(image_id);

CREATE TABLE IF NOT EXISTS dlq (
    job_id          UUID PRIMARY KEY,
    error           TEXT NOT NULL,
    attempts        INTEGER NOT NULL,
    moved_at        TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
