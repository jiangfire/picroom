// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Picroom Contributors

//! User entity.

use serde::{Deserialize, Serialize};
use std::str::FromStr;
use time::OffsetDateTime;
use uuid::Uuid;

/// Stable user identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct UserId(pub Uuid);

impl UserId {
    /// Returns the underlying UUID.
    pub const fn as_uuid(&self) -> Uuid {
        self.0
    }
    /// Nil user id (placeholder).
    pub const fn nil() -> Self {
        Self(Uuid::nil())
    }
}

impl std::fmt::Display for UserId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<Uuid> for UserId {
    fn from(u: Uuid) -> Self {
        Self(u)
    }
}

impl FromStr for UserId {
    type Err = uuid::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::from_str(s)?))
    }
}

/// User entity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct User {
    /// Stable id.
    pub id: UserId,
    /// Email address (lowercased).
    pub email: String,
    /// Display name.
    pub name: String,
    /// Optional avatar URL.
    pub avatar_url: Option<String>,
    /// Global role.
    pub role: String,
    /// Account creation timestamp.
    pub created_at: OffsetDateTime,
    /// Whether the user is disabled.
    pub disabled: bool,
}

/// User creation request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewUser {
    /// Email.
    pub email: String,
    /// Name.
    pub name: String,
    /// Argon2id-hashed password.
    pub password_hash: String,
    /// Initial role (default: `viewer`).
    pub role: String,
}

impl NewUser {
    /// Validates the email.
    pub fn validate_email(email: &str) -> Result<(), &'static str> {
        if !email.contains('@') || email.len() < 3 || email.len() > 254 {
            return Err("invalid email length");
        }
        let (local, domain) = email.split_once('@').ok_or("missing @")?;
        if local.is_empty() || domain.is_empty() || !domain.contains('.') {
            return Err("invalid email shape");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_email_accepts_normal() {
        assert!(NewUser::validate_email("alice@example.com").is_ok());
    }

    #[test]
    fn validate_email_rejects_missing_at() {
        assert!(NewUser::validate_email("alice.example.com").is_err());
    }

    #[test]
    fn validate_email_rejects_no_dot() {
        assert!(NewUser::validate_email("alice@example").is_err());
    }
}
