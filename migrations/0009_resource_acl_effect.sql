-- Resource ACL explicit deny (D-9): add an `effect` column so a deny is
-- expressible in the same table, and widen the UNIQUE constraint to include
-- it. Existing rows default to 'allow', keeping every prior grant meaningful.
-- Forward-only: no down migration ships.

ALTER TABLE resource_acls ADD COLUMN effect VARCHAR(6) NOT NULL DEFAULT 'allow';

ALTER TABLE resource_acls DROP CONSTRAINT IF EXISTS resource_acls_effect_check;
ALTER TABLE resource_acls ADD CONSTRAINT resource_acls_effect_check
    CHECK (effect IN ('allow', 'deny'));

-- The old UNIQUE (resource_type, resource_id, subject_type, subject_id,
-- permission) constraint cannot express allow+deny for the same tuple.
ALTER TABLE resource_acls
    DROP CONSTRAINT IF EXISTS resource_acls_resource_type_resource_id_subject_type_subject_id_permission_key;
ALTER TABLE resource_acls
    ADD CONSTRAINT resource_acls_grant_unique
    UNIQUE (resource_type, resource_id, subject_type, subject_id, permission, effect);
