// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Picroom Contributors

//! Quota service.
//!
//! ⚠️ **DEFERRED STUB.** This service currently reports unlimited quota
//! (`remaining_user` → `u64::MAX`, `charge_user` → `Ok(())`) and performs no
//! enforcement. Per-user/per-team byte caps are not checked before uploads.
//! This is an accepted v0.1 limitation — see `docs/review-2026-07.md` §3.3.
//! A real implementation requires a `quotas` table and pre-upload checks in
//! `UploadService`.

use crate::ServiceError;
use uuid::Uuid;

/// Quota service (deferred stub — see module docs).
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
