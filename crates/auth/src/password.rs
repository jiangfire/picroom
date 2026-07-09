// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Picroom Contributors

//! Argon2id password hashing.

use argon2::{
    password_hash::SaltString, Argon2, PasswordHash, PasswordHasher as _, PasswordVerifier,
};
use rand_core::OsRng;
use thiserror::Error;

/// Password hashing errors.
#[derive(Debug, Error)]
pub enum PasswordError {
    /// Hashing failed.
    #[error("hash error: {0}")]
    Hash(String),
    /// Verification failed.
    #[error("verify error: {0}")]
    Verify(String),
}

/// Argon2id password hasher.
#[derive(Debug, Clone, Default)]
pub struct PasswordHasher;

impl PasswordHasher {
    /// Creates a new hasher.
    pub const fn new() -> Self {
        Self
    }

    /// Hashes the password with a random salt.
    pub fn hash(&self, password: &str) -> Result<String, PasswordError> {
        let salt = SaltString::generate(&mut OsRng);
        let argon2 = Argon2::default();
        argon2
            .hash_password(password.as_bytes(), &salt)
            .map(|h| h.to_string())
            .map_err(|e| PasswordError::Hash(e.to_string()))
    }

    /// Verifies a password against a stored hash.
    pub fn verify(&self, password: &str, hash: &str) -> Result<bool, PasswordError> {
        let parsed = PasswordHash::new(hash).map_err(|e| PasswordError::Verify(e.to_string()))?;
        Ok(Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_then_verify_succeeds() {
        let h = PasswordHasher::new()
            .hash("correct horse battery staple")
            .unwrap();
        assert!(PasswordHasher::new()
            .verify("correct horse battery staple", &h)
            .unwrap());
        assert!(!PasswordHasher::new().verify("wrong", &h).unwrap());
    }

    #[test]
    fn hash_is_unique_per_call() {
        let h1 = PasswordHasher::new().hash("p").unwrap();
        let h2 = PasswordHasher::new().hash("p").unwrap();
        assert_ne!(h1, h2, "salts must differ");
    }
}
