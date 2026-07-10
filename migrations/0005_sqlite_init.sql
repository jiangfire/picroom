-- SQLite-compatible jobs + images + users + teams schema (subset).
-- Used by integration tests and the worker / admin binaries in dev mode.

CREATE TABLE IF NOT EXISTS users (
    id              TEXT PRIMARY KEY,
    email           TEXT NOT NULL UNIQUE,
    name            TEXT NOT NULL,
    password_hash   TEXT NOT NULL,
    role            TEXT NOT NULL DEFAULT 'viewer'
                       CHECK (role IN ('viewer', 'uploader', 'manager', 'admin')),
    avatar_url      TEXT,
    disabled        INTEGER NOT NULL DEFAULT 0,
    email_verified  INTEGER NOT NULL DEFAULT 0,
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS teams (
    id              TEXT PRIMARY KEY,
    name            TEXT NOT NULL,
    slug            TEXT NOT NULL UNIQUE,
    description     TEXT,
    storage_policy  TEXT,
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS team_members (
    team_id         TEXT NOT NULL REFERENCES teams(id) ON DELETE CASCADE,
    user_id         TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role            TEXT NOT NULL DEFAULT 'uploader'
                       CHECK (role IN ('viewer', 'uploader', 'manager', 'admin')),
    joined_at       TEXT NOT NULL,
    PRIMARY KEY (team_id, user_id)
);

CREATE TABLE IF NOT EXISTS images (
    id              TEXT PRIMARY KEY,
    owner_id        TEXT NOT NULL,
    team_id         TEXT,
    storage_policy  TEXT NOT NULL,
    storage_key     TEXT NOT NULL,
    content_type    TEXT NOT NULL,
    bytes           INTEGER NOT NULL CHECK (bytes >= 0),
    width           INTEGER NOT NULL DEFAULT 0 CHECK (width >= 0),
    height          INTEGER NOT NULL DEFAULT 0 CHECK (height >= 0),
    sha256          TEXT,
    status          TEXT NOT NULL DEFAULT 'pending',
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_images_owner ON images(owner_id);
CREATE INDEX IF NOT EXISTS idx_images_team ON images(team_id);

CREATE TABLE IF NOT EXISTS image_variants (
    id              TEXT PRIMARY KEY,
    image_id        TEXT NOT NULL REFERENCES images(id) ON DELETE CASCADE,
    kind            TEXT NOT NULL,
    size            INTEGER,
    storage_key     TEXT NOT NULL,
    bytes           INTEGER NOT NULL CHECK (bytes >= 0),
    content_type    TEXT NOT NULL,
    created_at      TEXT NOT NULL,
    UNIQUE (image_id, kind, size)
);

CREATE TABLE IF NOT EXISTS jobs (
    id              TEXT PRIMARY KEY,
    image_id        TEXT NOT NULL,
    kind            TEXT NOT NULL,
    payload         TEXT,
    status          TEXT NOT NULL DEFAULT 'pending',
    attempts        INTEGER NOT NULL DEFAULT 0,
    last_error      TEXT,
    enqueued_at     TEXT NOT NULL,
    started_at      TEXT,
    finished_at     TEXT
);

CREATE INDEX IF NOT EXISTS idx_jobs_pending ON jobs(enqueued_at)
    WHERE status = 'pending';

CREATE TABLE IF NOT EXISTS dlq (
    job_id          TEXT PRIMARY KEY,
    error           TEXT NOT NULL,
    attempts        INTEGER NOT NULL,
    moved_at        TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS audit_events (
    id              TEXT PRIMARY KEY,
    timestamp       TEXT NOT NULL,
    actor_id        TEXT,
    actor_label     TEXT,
    action          TEXT NOT NULL,
    target_type     TEXT NOT NULL,
    target_id       TEXT,
    ip              TEXT,
    user_agent      TEXT,
    metadata        TEXT NOT NULL DEFAULT '{}'
);

CREATE INDEX IF NOT EXISTS idx_audit_timestamp ON audit_events(timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_audit_actor ON audit_events(actor_id);

CREATE TABLE IF NOT EXISTS quotas (
    user_id   TEXT PRIMARY KEY,
    max_bytes INTEGER NOT NULL DEFAULT 1073741824
);