// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Picroom Contributors

//! Clock trait + impls.

use time::OffsetDateTime;

/// Trait for time injection (helpful in tests).
pub trait Clock: Send + Sync {
    /// Returns the current UTC time.
    fn now(&self) -> OffsetDateTime;
}

/// System clock (production).
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> OffsetDateTime {
        OffsetDateTime::now_utc()
    }
}

/// Fake clock (tests).
#[cfg(test)]
#[derive(Debug)]
pub struct FakeClock {
    time: std::sync::Mutex<OffsetDateTime>,
}

#[cfg(test)]
impl FakeClock {
    /// Creates a fake clock set to `time`.
    pub fn new(time: OffsetDateTime) -> Self {
        Self {
            time: std::sync::Mutex::new(time),
        }
    }

    /// Advances the clock by `seconds`.
    pub fn advance(&self, seconds: i64) {
        let mut t = self.time.lock().expect("mutex poisoned");
        *t += time::Duration::seconds(seconds);
    }
}

#[cfg(test)]
impl Default for FakeClock {
    fn default() -> Self {
        Self::new(OffsetDateTime::UNIX_EPOCH)
    }
}

#[cfg(test)]
impl Clock for FakeClock {
    fn now(&self) -> OffsetDateTime {
        *self.time.lock().expect("mutex poisoned")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_clock_returns_recent_time() {
        let c = SystemClock;
        let t = c.now();
        let now = OffsetDateTime::now_utc();
        let diff = (now - t).whole_seconds().abs();
        assert!(diff < 5, "system clock should be within 5s of real time");
    }

    #[test]
    fn fake_clock_advances() {
        let c = FakeClock::default();
        let t0 = c.now();
        c.advance(60);
        assert_eq!((c.now() - t0).whole_seconds(), 60);
    }
}
