//! Clock trait and implementations.

use picroom_domain::Clock as _;
use time::OffsetDateTime;

/// System clock (production).
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemClock;

impl picroom_domain::Clock for SystemClock {
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
impl picroom_domain::Clock for FakeClock {
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
        let t = picroom_domain::Clock::now(&c);
        let now = OffsetDateTime::now_utc();
        let diff = (now - t).whole_seconds().abs();
        assert!(diff < 5);
    }

    #[test]
    fn fake_clock_advances() {
        let c = FakeClock::default();
        let t0 = picroom_domain::Clock::now(&c);
        c.advance(60);
        assert_eq!((picroom_domain::Clock::now(&c) - t0).whole_seconds(), 60);
    }
}
