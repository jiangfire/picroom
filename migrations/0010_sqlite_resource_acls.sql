-- SQLite dev-path mirror of `resource_acls` (D-11): the same grant table, in
-- the SQLite subset, including the `effect` column from 0009. On PostgreSQL
-- this file is a no-op (0002 + 0009 already created the table in its final
-- shape and the index name below already exists).

CREATE TABLE IF NOT EXISTS resource_acls (
    id              TEXT PRIMARY KEY,
    resource_type   TEXT NOT NULL,
    resource_id     TEXT NOT NULL,
    subject_type    TEXT NOT NULL CHECK (subject_type IN ('user', 'team')),
    subject_id      TEXT NOT NULL,
    permission      TEXT NOT NULL
                       CHECK (permission IN ('read', 'create', 'update', 'delete', 'admin')),
    effect          TEXT NOT NULL DEFAULT 'allow' CHECK (effect IN ('allow', 'deny')),
    granted_at      TEXT NOT NULL,
    UNIQUE (resource_type, resource_id, subject_type, subject_id, permission, effect)
);

CREATE INDEX IF NOT EXISTS idx_resource_acls_subject
    ON resource_acls(subject_type, subject_id);
