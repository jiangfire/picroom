// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Picroom Contributors

//! Team entity.

use crate::user::UserId;
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use time::OffsetDateTime;
use uuid::Uuid;

/// Stable team identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TeamId(pub Uuid);

impl TeamId {
    /// Returns the underlying UUID.
    pub const fn as_uuid(&self) -> Uuid {
        self.0
    }
    /// Nil team id (placeholder).
    pub const fn nil() -> Self {
        Self(Uuid::nil())
    }
}

impl std::fmt::Display for TeamId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<Uuid> for TeamId {
    fn from(u: Uuid) -> Self {
        Self(u)
    }
}

impl FromStr for TeamId {
    type Err = uuid::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::from_str(s)?))
    }
}

/// Team entity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Team {
    /// Stable id.
    pub id: TeamId,
    /// Display name.
    pub name: String,
    /// URL-safe slug.
    pub slug: String,
    /// Optional description.
    pub description: Option<String>,
    /// Storage policy used by this team (if different from default).
    pub storage_policy: Option<String>,
    /// Creation timestamp.
    pub created_at: OffsetDateTime,
}

/// A team member.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TeamMember {
    /// Team id.
    pub team_id: TeamId,
    /// User id.
    pub user_id: UserId,
    /// Role within this team (overrides global role).
    pub role: String,
    /// When the user joined.
    pub joined_at: OffsetDateTime,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn team_id_display() {
        let id = TeamId(Uuid::nil());
        assert_eq!(id.to_string(), "00000000-0000-0000-0000-000000000000");
    }
}
