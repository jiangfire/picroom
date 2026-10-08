# Temporary patch: pool.rs — log JoinError, widen panic guard over complete/fail.
p = 'crates/worker/src/pool.rs'
src = open(p, encoding='utf-8').read()

old = '''    while !flag.load(Ordering::Relaxed) {
        if set.is_empty() {
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            continue;
        }
        tokio::select! {
            _ = set.join_next() => {}
        }
    }'''
new = '''    while !flag.load(Ordering::Relaxed) {
        if set.is_empty() {
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            continue;
        }
        // A JoinError here means the slot task itself died (e.g. a panic that
        // escaped the per-job guard) — log it loudly instead of silently
        // shrinking the pool.
        if let Some(err) = set.join_next().await {
            if let Ok(inner) = err {
                let _ = inner;
            } else {
                tracing::error!("worker slot task died: {err}");
            }
        }
    }'''
assert old in src, 'run_until'
src = src.replace(old, new)

old = '''                        Ok(Some(job)) => {
                            // A panicking handler must kill the *job*, not
                            // the slot: catch the unwind and route it through
                            // the normal failure path (retry/DLQ). Without
                            // this, one bad job permanently kills the slot
                            // and `run_until` spins on an empty JoinSet.
                            let r = futures::FutureExt::catch_unwind(
                                std::panic::AssertUnwindSafe(handler(job.clone())),
                            )
                            .await
                            .unwrap_or_else(|panic| {
                                Err(format!("handler panicked: {panic:?}"))
                            });
                            match r {
                                Ok(result) => {
                                    if let Err(e) = queue.complete(job.id, &result).await {
                                        tracing::warn!("complete failed: {e}");
                                    }
                                }
                                Err(e) => {
                                    let exhausted = job.attempts >= policy.max_attempts;
                                    if exhausted {
                                        let _ = dlq
                                            .push(DlqEntry {
                                                job_id: job.id,
                                                error: e.clone(),
                                                attempts: job.attempts,
                                                moved_at: OffsetDateTime::now_utc(),
                                            })
                                            .await;
                                    }
                                    let _ = queue.fail(job.id, &e).await;
                                    // Back off before the next dequeue so a
                                    // failing job is not retried instantly.
                                    // `delay_secs` is indexed by the attempt
                                    // that just failed (dequeue already
                                    // incremented `attempts`). Skipped when the
                                    // job is exhausted (no retry pending).
                                    if !exhausted {
                                        let delay = std::time::Duration::from_secs(
                                            policy.delay_secs(job.attempts),
                                        );
                                        tokio::time::sleep(delay).await;
                                    }
                                }
                            }
                        }
                        Ok(None) => {'''
new = '''                        Ok(Some(job)) => {
                            // A panicking handler must kill the *job*, not the
                            // slot: catch the unwind and route it through the
                            // normal failure path (retry/DLQ). The guard covers
                            // complete()/fail()/DLQ as well — a panic there
                            // would otherwise kill the slot (R-11).
                            let queue = queue.clone();
                            let dlq = dlq.clone();
                            let outcome = futures::FutureExt::catch_unwind(
                                std::panic::AssertUnwindSafe(async move {
                                    let r = handler(job.clone()).await;
                                    match r {
                                        Ok(result) => {
                                            if let Err(e) =
                                                queue.complete(job.id, &result).await
                                            {
                                                tracing::warn!("complete failed: {e}");
                                            }
                                        }
                                        Err(e) => {
                                            let exhausted =
                                                job.attempts >= policy.max_attempts;
                                            if exhausted {
                                                let _ = dlq
                                                    .push(DlqEntry {
                                                        job_id: job.id,
                                                        error: e.clone(),
                                                        attempts: job.attempts,
                                                        moved_at: OffsetDateTime::now_utc(),
                                                    })
                                                    .await;
                                            }
                                            let _ = queue.fail(job.id, &e).await;
                                            // Back off before the next dequeue
                                            // so a failing job is not retried
                                            // instantly. `delay_secs` is indexed
                                            // by the attempt that just failed
                                            // (dequeue already incremented
                                            // `attempts`). Skipped when the job
                                            // is exhausted (no retry pending).
                                            if !exhausted {
                                                let delay = std::time::Duration::from_secs(
                                                    policy.delay_secs(job.attempts),
                                                );
                                                tokio::time::sleep(delay).await;
                                            }
                                        }
                                    }
                                }),
                            )
                            .await;
                            if outcome.is_err() {
                                tracing::error!(
                                    job_id = %job.id,
                                    "job handling panicked (complete/fail path); slot survives"
                                );
                            }
                        }
                        Ok(None) => {'''
assert old in src, 'slot body'
src = src.replace(old, new)
open(p, 'w', encoding='utf-8', newline='\n').write(src)
print("ok: pool")
