// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Picroom Contributors

//! Fixed-window rate limiting for the unauthenticated auth endpoints.
//!
//! `POST /api/v1/auth/login` and the OIDC start endpoint are reachable without
//! credentials, so they are the only realistic brute-force surface: an attacker
//! can guess passwords, or spray one password across many accounts. Counting
//! them per client IP and per account closes both — the IP bucket stops one
//! host from spraying, the account bucket stops a distributed spray against a
//! single account.
//!
//! In-process and per-instance by design (D-16): a horizontally scaled
//! deployment is bounded per replica, which is the same trade-off the reverse
//! proxy already covers with a shared store.

use std::collections::HashMap;
use std::sync::Mutex;
use time::OffsetDateTime;

/// Once this many keys are tracked the map is swept of expired windows, so a
/// spray of distinct source addresses cannot grow it without bound.
const SWEEP_THRESHOLD: usize = 4096;

/// Counters for one fixed window.
#[derive(Debug, Clone, Copy)]
struct Window {
    /// Attempts seen in this window.
    count: u32,
    /// When the window rolls over.
    reset_at: OffsetDateTime,
}

/// A fixed-window attempt counter shared by the auth endpoints.
#[derive(Debug)]
pub struct AuthRateLimiter {
    /// Attempts allowed per window. `0` disables limiting entirely.
    max_attempts: u32,
    /// Window length in seconds.
    window_secs: i64,
    /// Keyed by `ip:<addr>` and `acct:<email>`; the prefixes keep a source
    /// address from colliding with an account name.
    windows: Mutex<HashMap<String, Window>>,
}

impl AuthRateLimiter {
    /// Builds a limiter allowing `max_attempts` per `window_secs`.
    ///
    /// `max_attempts == 0` disables limiting, which is what dev and test
    /// wiring uses so that suites making many logins are not throttled.
    #[must_use]
    pub fn new(max_attempts: u32, window_secs: u64) -> Self {
        Self {
            max_attempts,
            window_secs: i64::try_from(window_secs).unwrap_or(i64::MAX),
            windows: Mutex::new(HashMap::new()),
        }
    }

    /// A limiter that never blocks.
    #[must_use]
    pub fn disabled() -> Self {
        Self::new(0, 0)
    }

    /// Counts one attempt against both the client-IP and the account bucket,
    /// returning how long the caller must wait when either is exhausted.
    ///
    /// `None` means the attempt is allowed to proceed. Both buckets are counted
    /// even when one is already exhausted, so an attacker cannot escape the
    /// account budget by first burning the IP budget.
    pub fn record(&self, ip: &str, account: &str) -> Option<u64> {
        self.record_at(OffsetDateTime::now_utc(), ip, account)
    }

    /// [`Self::record`] against an explicit clock, so window rollover can be
    /// tested without sleeping.
    fn record_at(&self, now: OffsetDateTime, ip: &str, account: &str) -> Option<u64> {
        if self.max_attempts == 0 {
            return None;
        }
        let span = time::Duration::seconds(self.window_secs);
        let mut windows = self.windows.lock().expect("rate limit mutex poisoned");
        let mut blocked_for: Option<i64> = None;

        for key in [format!("ip:{ip}"), format!("acct:{account}")] {
            let window = windows.entry(key).or_insert(Window {
                count: 0,
                reset_at: now + span,
            });
            if now >= window.reset_at {
                *window = Window {
                    count: 0,
                    reset_at: now + span,
                };
            }
            window.count += 1;
            if window.count > self.max_attempts {
                let remaining = (window.reset_at - now).whole_seconds();
                blocked_for = Some(blocked_for.map_or(remaining, |seen| seen.max(remaining)));
            }
        }

        if windows.len() > SWEEP_THRESHOLD {
            windows.retain(|_, window| now < window.reset_at);
        }

        // `Retry-After` is whole seconds and must never be zero, or a client
        // that retries immediately would spin.
        blocked_for.map(|secs| u64::try_from(secs).unwrap_or(1).max(1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::Duration;

    fn at(secs: i64) -> OffsetDateTime {
        OffsetDateTime::UNIX_EPOCH + Duration::seconds(secs)
    }

    #[test]
    fn allows_attempts_up_to_the_threshold_then_blocks() {
        let limiter = AuthRateLimiter::new(3, 900);
        let now = at(1_000);
        for _ in 0..3 {
            assert_eq!(limiter.record_at(now, "10.0.0.1", "a@example.com"), None);
        }
        assert_eq!(
            limiter.record_at(now, "10.0.0.1", "a@example.com"),
            Some(900),
            "the fourth attempt exceeds the threshold"
        );
    }

    #[test]
    fn retry_after_shrinks_as_the_window_elapses() {
        let limiter = AuthRateLimiter::new(1, 600);
        assert_eq!(limiter.record_at(at(0), "10.0.0.1", "a@example.com"), None);
        assert_eq!(
            limiter.record_at(at(120), "10.0.0.1", "a@example.com"),
            Some(480),
            "the caller is told the remaining window, not the full one"
        );
    }

    #[test]
    fn window_resets_once_it_expires() {
        let limiter = AuthRateLimiter::new(2, 60);
        let start = at(1_000);
        assert_eq!(limiter.record_at(start, "10.0.0.1", "a@example.com"), None);
        assert_eq!(limiter.record_at(start, "10.0.0.1", "a@example.com"), None);
        assert!(limiter
            .record_at(start, "10.0.0.1", "a@example.com")
            .is_some());

        // One second past the window the budget is whole again.
        let later = start + Duration::seconds(61);
        assert_eq!(limiter.record_at(later, "10.0.0.1", "a@example.com"), None);
        assert_eq!(limiter.record_at(later, "10.0.0.1", "a@example.com"), None);
        assert!(limiter
            .record_at(later, "10.0.0.1", "a@example.com")
            .is_some());
    }

    #[test]
    fn separate_clients_have_separate_budgets() {
        let limiter = AuthRateLimiter::new(2, 900);
        let now = at(0);
        // Exhaust 10.0.0.1's budget on account a@example.com.
        assert_eq!(limiter.record_at(now, "10.0.0.1", "a@example.com"), None);
        assert_eq!(limiter.record_at(now, "10.0.0.1", "a@example.com"), None);
        assert!(limiter
            .record_at(now, "10.0.0.1", "a@example.com")
            .is_some());

        assert_eq!(
            limiter.record_at(now, "10.0.0.2", "b@example.com"),
            None,
            "a different client must not inherit the first one's exhausted IP budget"
        );
    }

    #[test]
    fn one_client_cannot_spray_many_accounts() {
        let limiter = AuthRateLimiter::new(2, 900);
        let now = at(0);
        assert_eq!(limiter.record_at(now, "10.0.0.1", "a@example.com"), None);
        assert_eq!(limiter.record_at(now, "10.0.0.1", "b@example.com"), None);
        assert!(
            limiter
                .record_at(now, "10.0.0.1", "c@example.com")
                .is_some(),
            "the per-IP budget is shared across accounts"
        );
    }

    #[test]
    fn one_account_is_bounded_across_clients() {
        let limiter = AuthRateLimiter::new(2, 900);
        let now = at(0);
        assert_eq!(limiter.record_at(now, "10.0.0.1", "a@example.com"), None);
        assert_eq!(limiter.record_at(now, "10.0.0.2", "a@example.com"), None);
        assert!(
            limiter
                .record_at(now, "10.0.0.3", "a@example.com")
                .is_some(),
            "a distributed spray against one account is still bounded"
        );
    }

    #[test]
    fn exhausted_ip_budget_does_not_refill_the_account_budget() {
        let limiter = AuthRateLimiter::new(1, 900);
        let now = at(0);
        assert_eq!(limiter.record_at(now, "10.0.0.1", "a@example.com"), None);
        // Blocked on the IP side, but the account must keep counting so the
        // attacker cannot reset the per-account budget by burning the IP one.
        assert!(limiter
            .record_at(now, "10.0.0.1", "a@example.com")
            .is_some());
        assert!(limiter
            .record_at(now, "10.0.0.1", "a@example.com")
            .is_some());
    }

    #[test]
    fn zero_max_attempts_never_blocks() {
        let limiter = AuthRateLimiter::new(0, 900);
        let now = at(0);
        for _ in 0..100 {
            assert_eq!(limiter.record_at(now, "10.0.0.1", "a@example.com"), None);
        }
    }

    #[test]
    fn account_and_ip_keys_cannot_collide() {
        let limiter = AuthRateLimiter::new(1, 900);
        let now = at(0);
        // "ip:..." as an account name must not land in the IP bucket.
        assert_eq!(limiter.record_at(now, "10.0.0.1", "ip:10.0.0.2"), None);
        assert_eq!(
            limiter.record_at(now, "10.0.0.2", "b@example.com"),
            None,
            "the IP bucket counted only its own address"
        );
    }
}
