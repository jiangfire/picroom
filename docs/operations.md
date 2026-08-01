# Operations

Day-2 procedures for a running Picroom deployment. All commands assume repo
root or the binary on `PATH`.

## 1. Migrations

```bash
picroom admin migrate run      # apply pending migrations (idempotent)
picroom admin migrate status   # list applied / pending / failed migrations
```

`migrate status` reads the embedded migration set against the
`_sqlx_migrations` table and reports applied, pending, and failed rows.
`migrate revert` is **not** supported (no down-migrations are shipped — the
binary returns a clear error). To inspect state manually:

```sql
SELECT version, description, success FROM _sqlx_migrations ORDER BY version;
```

Migrations live in `migrations/*.sql` and are embedded into the binary via
`sqlx::migrate!`.

## 2. User & team administration

```bash
picroom admin user create --email alice@example.com --name Alice --role admin --password '…'
picroom admin user list
picroom admin user set-role <uuid> --role manager
picroom admin user disable <uuid>
```

Both PostgreSQL (`user_create_pg`, `user_list_pg`, `user_set_role_pg`,
`user_disable_pg`) and SQLite (`user_create_sqlite`, …) paths are wired in
`crates/admin/src/user.rs`; the CLI auto-selects the implementation from the
`Database` enum.

## 3. Storage health check

```bash
picroom admin storage-test --policy default    # put/get/delete round-trip
```

Readiness reflects live DB + storage probes:

```bash
curl -fsS http://localhost:8080/readyz
# {"status":"ready","checks":{"database":true,"database_configured":true,"storage":true}}
```

A `503` with `"not_ready"` names which dependency failed.

## 4. Audit log

Every state-changing API call (upload, delete, login, role change) writes an
audit event via `DbAuditSink` to the `audit_events` table (PostgreSQL path).
Query directly:

```sql
SELECT timestamp, actor_label, action, target_type, target_id
FROM audit_events ORDER BY timestamp DESC LIMIT 50;
```

`admin audit tail` reads the `audit_events` table for both PostgreSQL and
SQLite (see `admin/src/audit_cmd.rs`); pass `--follow` to stream new events
as they arrive. An optional `--actor <email>` filters by actor label.

## 5. Backup & restore

- **PostgreSQL**: `pg_dump -Fc picroom > picroom-$(date +%F).dump`. Restore
  with `pg_restore -d picroom`. This covers metadata, variants, jobs, audit.
- **Objects**: back up the S3/MinIO bucket separately (`aws s3 sync` or MinIO
  bucket replication). `LocalDriver` data under `./data` must be backed up at
  the filesystem level.

Restore order: restore PostgreSQL, then ensure the object store contains the
referenced keys. Mismatches (DB row without object) surface as `404`/`500` on
read.

## 6. Upgrades

1. Back up PostgreSQL + objects (§5).
2. Pull the new image / rebuild the binary.
3. Apply migrations: `picroom admin migrate run`. Compose does this
   automatically via the `migrate` init container.
4. Restart `api` and `worker`.

The worker applies exponential backoff between retries and moves jobs to a DLQ
after `max_attempts`; inspect the `jobs` table (`status = 'dead'`) for poison
messages and `image_variants` for completed work.

## 7. Metrics

`GET /metrics` exposes Prometheus exposition format (counters, gauges,
histograms registered in `picroom_infra::telemetry`). Scrape it with a
standard Prometheus job. Key series: `picroom_http_requests_total`,
`picroom_uploads_total`, `picroom_upload_duration_seconds`,
`picroom_worker_queue_depth`.

## 8. Graceful shutdown

Both `api` and `worker` drain on `SIGTERM`/`SIGINT`. For rolling restarts,
wait for the process to exit 0 before removing the instance from the
load balancer.
