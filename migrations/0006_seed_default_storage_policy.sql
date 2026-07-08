-- Seed a default storage policy.
--
-- PgImageRepository::insert hardcodes storage_policy = 'default', which
-- FK-references storage_policies(name). The schema migrations never create a
-- policy row, so without this seed every image insert fails with a foreign-key
-- violation. This makes the application self-bootstrapping instead of relying
-- on out-of-band seeding. Idempotent, so it is safe to re-run.
INSERT INTO storage_policies (name, driver, config, is_default)
VALUES ('default', 'local', '{}', true)
ON CONFLICT (name) DO NOTHING;
