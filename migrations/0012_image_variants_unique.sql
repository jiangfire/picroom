-- Variant upsert idempotency (R-18 / D-5): Postgres treats NULLs as distinct,
-- so `ON CONFLICT (image_id, kind, size)` never fired for avif/webp rows
-- (size IS NULL) and re-running a job duplicated rows. Replace the NULL-blind
-- constraint with an expression index over COALESCE(size, -1). Portable SQL
-- (SQLite supports expression indexes too).

-- 1. Dedupe existing rows (keep the oldest of each natural key). `id` is
-- UUID on PostgreSQL and has no MIN aggregate there, so order by the text
-- form — portable across both dialects and stable enough for "keep one".
DELETE FROM image_variants
WHERE id::text NOT IN (
    SELECT MIN(id::text) FROM image_variants
    GROUP BY image_id, kind, COALESCE(size, -1)
);

-- 2. Enforce the natural key from now on.
CREATE UNIQUE INDEX IF NOT EXISTS image_variants_natural_key
    ON image_variants (image_id, kind, COALESCE(size, -1));
