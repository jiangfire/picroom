-- Storage policies + image metadata
-- Phase 2.

CREATE TABLE IF NOT EXISTS storage_policies (
    name            VARCHAR(64) PRIMARY KEY,
    driver          VARCHAR(32) NOT NULL
                       CHECK (driver IN ('local', 's3', 'oss', 'cos', 'qiniu', 'minio')),
    config          JSONB NOT NULL,
    is_default      BOOLEAN NOT NULL DEFAULT FALSE,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_storage_policies_default
    ON storage_policies (is_default) WHERE is_default = TRUE;

CREATE TABLE IF NOT EXISTS images (
    id              UUID PRIMARY KEY,
    owner_id        UUID NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    team_id         UUID REFERENCES teams(id) ON DELETE SET NULL,
    storage_policy  VARCHAR(64) NOT NULL REFERENCES storage_policies(name) ON DELETE RESTRICT,
    storage_key     VARCHAR(1024) NOT NULL,
    content_type    VARCHAR(127) NOT NULL,
    bytes           BIGINT NOT NULL CHECK (bytes >= 0),
    width           INTEGER NOT NULL DEFAULT 0 CHECK (width >= 0),
    height          INTEGER NOT NULL DEFAULT 0 CHECK (height >= 0),
    sha256          CHAR(64),
    status          VARCHAR(32) NOT NULL DEFAULT 'pending'
                       CHECK (status IN ('pending', 'ready', 'failed', 'deleted')),
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_images_owner ON images(owner_id);
CREATE INDEX IF NOT EXISTS idx_images_team ON images(team_id);
CREATE INDEX IF NOT EXISTS idx_images_created ON images(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_images_sha256 ON images(sha256);

CREATE TABLE IF NOT EXISTS image_variants (
    id              UUID PRIMARY KEY,
    image_id        UUID NOT NULL REFERENCES images(id) ON DELETE CASCADE,
    kind            VARCHAR(32) NOT NULL
                       CHECK (kind IN ('avif', 'webp', 'thumbnail', 'watermark')),
    size            INTEGER,
    storage_key     VARCHAR(1024) NOT NULL,
    bytes           BIGINT NOT NULL CHECK (bytes >= 0),
    content_type    VARCHAR(127) NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (image_id, kind, size)
);

CREATE INDEX IF NOT EXISTS idx_image_variants_image ON image_variants(image_id);

CREATE TABLE IF NOT EXISTS resource_acls (
    id              UUID PRIMARY KEY,
    resource_type   VARCHAR(64) NOT NULL,
    resource_id     UUID NOT NULL,
    subject_type    VARCHAR(32) NOT NULL
                       CHECK (subject_type IN ('user', 'team')),
    subject_id      UUID NOT NULL,
    permission      VARCHAR(32) NOT NULL
                       CHECK (permission IN ('read', 'create', 'update', 'delete', 'admin')),
    granted_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (resource_type, resource_id, subject_type, subject_id, permission)
);

CREATE INDEX IF NOT EXISTS idx_resource_acls_subject
    ON resource_acls(subject_type, subject_id);