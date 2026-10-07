-- Variant upsert idempotency (R-18 / D-5): Postgres treats NULLs as distinct,
-- so `ON CONFLICT (image_id, kind, size)` never fired for avif/webp rows
-- (size IS NULL) and re-running a job duplicated rows. Replace the NULL-blind
-- constraint with an expression index over COALESCE(size, -1). Portable SQL
-- (SQLite supports expression indexes too).

-- 1. Dedupe existing rows (keep the oldest of each natural key).
DELETE FROM image_variants
WHERE id NOT IN (
    SELECT MIN(id) FROM image_variants
    GROUP BY image_id, kind, COALESCE(size, -1)
);

-- 2. Enforce the natural key from now on.
CREATE UNIQUE INDEX IF NOT EXISTS image_variants_natural_key
    ON image_variants (image_id, kind, COALESCE(size, -1));
