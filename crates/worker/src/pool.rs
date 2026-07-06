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
        stop: Arc<AtomicBool>,
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
                                    if job.attempts + 1 >= policy.max_attempts {
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
        stop: Arc<AtomicBool>,
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
    pool.run_into(flag_for_wait, handler, &mut set);

    // Drain tasks as they finish until the stop flag is set.
    while !flag.load(Ordering::Relaxed) {
        tokio::select! {
            _ = set.join_next() => {}
        }
    }

    while set.join_next().await.is_some() {}
}