-- Job leases (D-8): a worker killed mid-job leaves the row `running` forever
-- unless leases expire and another worker can reclaim it. Portable column
-- adds (both dialects accept TIMESTAMPTZ/TEXT as dynamic types).

ALTER TABLE jobs ADD COLUMN lease_expires_at TIMESTAMPTZ;
ALTER TABLE jobs ADD COLUMN claimed_by TEXT;

CREATE INDEX IF NOT EXISTS idx_jobs_lease ON jobs (status, lease_expires_at);
