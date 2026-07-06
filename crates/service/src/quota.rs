//! Quota service.

use crate::ServiceError;
use uuid::Uuid;

/// Quota service (skeleton).
#[derive(Debug, Clone)]
pub struct QuotaService;

impl QuotaService {
    /// Creates a new quota service.
    pub const fn new() -> Self {
        Self
    }

    /// Returns remaining bytes for the user.
    pub async fn remaining_user(&self, _user_id: Uuid) -> Result<u64, ServiceError> {
        Ok(u64::MAX)
    }

    /// Returns remaining bytes for the team.
    pub async fn remaining_team(&self, _team_id: Uuid) -> Result<u64, ServiceError> {
        Ok(u64::MAX)
    }

    /// Charges `bytes` against the user's quota; returns error if exceeded.
    pub async fn charge_user(&self, _user_id: Uuid, _bytes: u64) -> Result<(), ServiceError> {
        Ok(())
    }
}

impl Default for QuotaService {
    fn default() -> Self {
        Self::new()
    }
}
