// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Picroom Contributors

//! Retry policies.

use serde::{Deserialize, Serialize};

/// Retry strategy.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum RetryStrategy {
    /// No retry.
    None,
    /// Fixed delay.
    Fixed,
    /// Exponential backoff.
    Exponential,
    /// Linear backoff.
    Linear,
}

/// Retry policy configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryPolicy {
    /// Maximum attempts (including the first).
    pub max_attempts: u32,
    /// Initial delay (seconds).
    pub initial_delay_secs: u64,
    /// Maximum delay (seconds, for exponential).
    pub max_delay_secs: u64,
    /// Strategy.
    pub strategy: RetryStrategy,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 5,
            initial_delay_secs: 1,
            max_delay_secs: 60,
            strategy: RetryStrategy::Exponential,
        }
    }
}

impl RetryPolicy {
    /// Computes the delay for the given attempt (1-indexed).
    pub fn delay_secs(&self, attempt: u32) -> u64 {
        let base = self.initial_delay_secs;
        let delay = match self.strategy {
            RetryStrategy::None => return 0,
            RetryStrategy::Fixed => base,
            RetryStrategy::Linear => base.saturating_mul(u64::from(attempt)),
            RetryStrategy::Exponential => base.saturating_pow(attempt.min(10)),
        };
        delay.min(self.max_delay_secs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exponential_caps_at_max() {
        let p = RetryPolicy {
            max_attempts: 5,
            initial_delay_secs: 2,
            max_delay_secs: 60,
            strategy: RetryStrategy::Exponential,
        };
        assert_eq!(p.delay_secs(1), 2);
        assert_eq!(p.delay_secs(2), 4);
        assert_eq!(p.delay_secs(3), 8);
        assert_eq!(p.delay_secs(6), 60); // capped
    }
}
