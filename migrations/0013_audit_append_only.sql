-- Audit log is append-only (R-17 / S14): reject UPDATE and DELETE at the
-- database level so audit history cannot be rewritten through any code path.
-- PostgreSQL trigger; the SQLite dev path has no DB audit sink (NoopAudit),
-- so there is nothing there to guard.

CREATE OR REPLACE FUNCTION audit_events_append_only() RETURNS trigger AS $$
BEGIN
    RAISE EXCEPTION 'audit_events is append-only';
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS audit_events_no_update ON audit_events;
DROP TRIGGER IF EXISTS audit_events_no_delete ON audit_events;

CREATE TRIGGER audit_events_no_update
    BEFORE UPDATE ON audit_events
    FOR EACH STATEMENT EXECUTE FUNCTION audit_events_append_only();

CREATE TRIGGER audit_events_no_delete
    BEFORE DELETE ON audit_events
    FOR EACH STATEMENT EXECUTE FUNCTION audit_events_append_only();
