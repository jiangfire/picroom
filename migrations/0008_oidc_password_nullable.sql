-- OIDC-linked accounts have no local password, so `password_hash` must be
-- nullable. Identity is tracked in the existing `oidc_links` table
-- (provider, subject) → user_id, introduced in 0004_sessions_and_oidc.sql.
-- This is idempotent: dropping NOT NULL when already nullable is a no-op.

ALTER TABLE users ALTER COLUMN password_hash DROP NOT NULL;
