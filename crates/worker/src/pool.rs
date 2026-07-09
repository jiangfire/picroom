// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Picroom Contributors

//! Worker pool — concurrent job consumers.

use crate::dlq::{DlqEntry, DlqSink};
use crate::job::{Job, JobQueue, JobResult};
use crate::retry::RetryPolicy;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use time::OffsetDateTime;

/// Worker pool.
pub struct WorkerPool<Q: JobQueue, D: DlqSink> {
    queue: Arc<Q>,
    dlq: Arc<D>,
    policy: RetryPolicy,
    concurrency: usize,
}

impl<Q: JobQueue + 'static, D: DlqSink + 'static> WorkerPool<Q, D> {
    /// Creates a new pool.
    pub fn new(queue: Arc<Q>, dlq: Arc<D>, policy: RetryPolicy, concurrency: usize) -> Self {
        Self {
            queue,
            dlq,
            policy,
            concurrency: concurrency.max(1),
        }
    }

    /// Spawns one task per worker slot onto a `JoinSet`. The caller
    /// keeps the `JoinSet` alive for as long as the workers should run.
    pub fn run_into<Fut>(
        &self,
        stop: &Arc<AtomicBool>,
        handler: impl Fn(Job) -> Fut + Send + Sync + 'static + Clone,
        set: &mut tokio::task::JoinSet<()>,
    ) where
        Fut: std::future::Future<Output = Result<JobResult, String>> + Send,
    {
        for _ in 0..self.concurrency {
            let queue = self.queue.clone();
            let dlq = self.dlq.clone();
            let policy = self.policy.clone();
            let handler = handler.clone();
            let stop = stop.clone();
            set.spawn(async move {
                loop {
                    if stop.load(Ordering::Relaxed) {
                        break;
                    }
                    match queue.dequeue().await {
                        Ok(Some(job)) => {
                            let r = handler(job.clone()).await;
                            match r {
                                Ok(result) => {
                                    if let Err(e) = queue.complete(job.id, &result).await {
                                        tracing::warn!("complete failed: {e}");
                                    }
                                }
                                Err(e) => {
                                    let exhausted = job.attempts + 1 >= policy.max_attempts;
                                    if exhausted {
                                        let _ = dlq
                                            .push(DlqEntry {
                                                job_id: job.id,
                                                error: e.clone(),
                                                attempts: job.attempts + 1,
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
                        Ok(None) => {
                            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                        }
                        Err(e) => {
                            tracing::error!("dequeue error: {e}");
                            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                        }
                    }
                }
            });
        }
    }

    /// Convenience: spawn tasks via `tokio::spawn` and forget. Note that
    /// the runtime must have other pending tasks; otherwise the workers
    /// will exit immediately when this function returns.
    ///
    /// Prefer [`Self::run_into`] with a [`tokio::task::JoinSet`] in
    /// long-running binaries.
    #[allow(dead_code)]
    pub fn run<Fut>(
        &self,
        stop: &Arc<AtomicBool>,
        handler: impl Fn(Job) -> Fut + Send + Sync + 'static + Clone,
    ) where
        Fut: std::future::Future<Output = Result<JobResult, String>> + Send,
    {
        let mut set = tokio::task::JoinSet::new();
        self.run_into(stop, handler, &mut set);
        // Leak the JoinSet so tasks are detached; this matches the
        // historical fire-and-forget semantics. Real callers should use
        // `run_into` directly and own the JoinSet.
        std::mem::forget(set);
    }
}

/// Convenience: spawn the worker pool with a future-based stop signal.
pub async fn run_until<F, Fut, Q, D>(
    pool: &WorkerPool<Q, D>,
    stop_signal: F,
    handler: impl Fn(Job) -> Fut + Send + Sync + 'static + Clone,
) where
    F: std::future::Future<Output = ()> + Send + 'static,
    Fut: std::future::Future<Output = Result<JobResult, String>> + Send,
    Q: JobQueue + 'static,
    D: DlqSink + 'static,
{
    let flag = Arc::new(AtomicBool::new(false));
    let flag_for_wait = flag.clone();
    let flag_for_signal = flag.clone();
    tokio::spawn(async move {
        stop_signal.await;
        flag_for_signal.store(true, Ordering::Relaxed);
    });

    let mut set = tokio::task::JoinSet::new();
    pool.run_into(&flag_for_wait, handler, &mut set);

    // Drain tasks as they finish until the stop flag is set.
    while !flag.load(Ordering::Relaxed) {
        tokio::select! {
            _ = set.join_next() => {}
        }
    }

    while set.join_next().await.is_some() {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dlq::InMemoryDlq;
    use crate::job::{JobError, JobKind, JobResult};
    use crate::retry::RetryStrategy;
    use picroom_domain::ImageId;
    use std::sync::atomic::AtomicU32;
    use std::sync::Mutex;
    use uuid::Uuid;

    /// A single-job in-memory queue that mimics real dequeue semantics
    /// (increments `attempts` on each dequeue) and records dequeue timestamps.
    struct FakeQueue {
        attempts: AtomicU32,
        done: AtomicBool,
        times: Arc<Mutex<Vec<std::time::Instant>>>,
    }

    #[async_trait::async_trait]
    impl JobQueue for FakeQueue {
        async fn enqueue(&self, _job: Job) -> Result<(), JobError> {
            Ok(())
        }
        async fn dequeue(&self) -> Result<Option<Job>, JobError> {
            self.times
                .lock()
                .expect("mutex poisoned")
                .push(std::time::Instant::now());
            if self.done.load(Ordering::SeqCst) {
                return Ok(None);
            }
            let attempts = self.attempts.fetch_add(1, Ordering::SeqCst) + 1;
            Ok(Some(Job {
                id: Uuid::nil(),
                image_id: ImageId(Uuid::nil()),
                kind: JobKind::EncodeAvif,
                attempts,
                enqueued_at: OffsetDateTime::now_utc(),
            }))
        }
        async fn complete(&self, _id: Uuid, _result: &JobResult) -> Result<(), JobError> {
            Ok(())
        }
        async fn fail(&self, _id: Uuid, _error: &str) -> Result<(), JobError> {
            Ok(())
        }
    }

    /// Proves the worker sleeps for the retry-policy delay between a failed
    /// attempt and the next dequeue (regression test for instant-retry bug).
    #[tokio::test]
    async fn worker_applies_backoff_between_retries() {
        let times = Arc::new(Mutex::new(Vec::new()));
        let queue = Arc::new(FakeQueue {
            attempts: AtomicU32::new(0),
            done: AtomicBool::new(false),
            times: times.clone(),
        });
        let dlq = Arc::new(InMemoryDlq::new());
        let policy = RetryPolicy {
            max_attempts: 5,
            initial_delay_secs: 1,
            max_delay_secs: 60,
            strategy: RetryStrategy::Exponential,
        };
        let pool = WorkerPool::new(queue, dlq, policy, 1);

        let stop = Arc::new(AtomicBool::new(false));
        let calls = Arc::new(AtomicU32::new(0));
        let handler = {
            let stop = stop.clone();
            let calls = calls.clone();
            move |_job: Job| {
                let stop = stop.clone();
                let calls = calls.clone();
                async move {
                    // Fail the first attempt; stop after the second so the
                    // test observes exactly one backoff gap.
                    let n = calls.fetch_add(1, Ordering::SeqCst);
                    if n >= 1 {
                        stop.store(true, Ordering::SeqCst);
                    }
                    Err::<JobResult, String>("boom".into())
                }
            }
        };

        let mut set = tokio::task::JoinSet::new();
        pool.run_into(&stop, handler, &mut set);
        while set.join_next().await.is_some() {}

        let ts = times.lock().expect("mutex poisoned").clone();
        assert!(
            ts.len() >= 2,
            "expected at least 2 dequeues, got {}",
            ts.len()
        );
        // Gap between dequeue #1 (1st attempt) and #2 (2nd attempt) must
        // reflect the ~1s exponential backoff applied after the failure.
        let gap = ts[1].duration_since(ts[0]);
        assert!(
            gap >= std::time::Duration::from_millis(900),
            "expected backoff >= ~1s before retry, got {gap:?}"
        );
    }
}
