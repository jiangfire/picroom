// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Picroom Contributors

//! JWT issuing and verification.

use jsonwebtoken::{
    decode, encode, errors::ErrorKind as JwtErrorKind, DecodingKey, EncodingKey, Header, Validation,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use time::OffsetDateTime;

/// JWT claims.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JwtClaims {
    /// Subject (user id).
    pub sub: String,
    /// Issuer.
    pub iss: String,
    /// Audience.
    pub aud: String,
    /// Issued-at (seconds since epoch).
    pub iat: i64,
    /// Expiry (seconds since epoch).
    pub exp: i64,
    /// Optional scopes.
    #[serde(default)]
    pub scopes: Vec<String>,
}

/// JWT errors.
#[derive(Debug, Error)]
pub enum JwtError {
    /// Encoding failed.
    #[error("encode: {0}")]
    Encode(String),
    /// Decoding failed.
    #[error("decode: {0}")]
    Decode(String),
    /// Signature invalid.
    #[error("invalid signature")]
    InvalidSignature,
    /// Token expired.
    #[error("token expired")]
    Expired,
}

/// JWT service.
#[derive(Debug, Clone)]
pub struct JwtService {
    secret: String,
    issuer: String,
    audience: String,
    ttl_seconds: i64,
}

impl JwtService {
    /// Creates a new JWT service.
    pub fn new(
        secret: impl Into<String>,
        issuer: impl Into<String>,
        audience: impl Into<String>,
        ttl_seconds: i64,
    ) -> Self {
        Self {
            secret: secret.into(),
            issuer: issuer.into(),
            audience: audience.into(),
            ttl_seconds,
        }
    }

    /// Issues a JWT for `subject` with no scopes.
    ///
    /// Prefer [`Self::issue_with_scopes`] for interactive login so the token
    /// carries the user's role(s); `AuthUser` relies on `scopes` to reconstruct
    /// [`crate::Role`]s.
    pub fn issue(&self, subject: impl Into<String>) -> Result<String, JwtError> {
        self.issue_with_scopes(subject, &[])
    }

    /// Issues a JWT for `subject` carrying the given `scopes` (e.g. role names).
    pub fn issue_with_scopes(
        &self,
        subject: impl Into<String>,
        scopes: &[String],
    ) -> Result<String, JwtError> {
        let now = OffsetDateTime::now_utc().unix_timestamp();
        let claims = JwtClaims {
            sub: subject.into(),
            iss: self.issuer.clone(),
            aud: self.audience.clone(),
            iat: now,
            exp: now + self.ttl_seconds,
            scopes: scopes.to_vec(),
        };
        encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(self.secret.as_bytes()),
        )
        .map_err(|e| JwtError::Encode(e.to_string()))
    }

    /// Verifies and decodes a JWT.
    pub fn verify(&self, token: &str) -> Result<JwtClaims, JwtError> {
        let mut validation = Validation::default();
        validation.set_audience(&[&self.audience]);
        validation.set_issuer(&[&self.issuer]);
        match decode::<JwtClaims>(
            token,
            &DecodingKey::from_secret(self.secret.as_bytes()),
            &validation,
        ) {
            Ok(data) => Ok(data.claims),
            Err(e) => match e.kind() {
                JwtErrorKind::ExpiredSignature => Err(JwtError::Expired),
                JwtErrorKind::InvalidSignature => Err(JwtError::InvalidSignature),
                _ => Err(JwtError::Decode(e.to_string())),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn issue_then_verify_roundtrip() {
        let s = JwtService::new("secret", "iss", "aud", 60);
        let token = s.issue("user-1").unwrap();
        let claims = s.verify(&token).unwrap();
        assert_eq!(claims.sub, "user-1");
        assert_eq!(claims.iss, "iss");
        assert_eq!(claims.aud, "aud");
    }

    #[test]
    fn verify_rejects_wrong_secret() {
        let s1 = JwtService::new("a", "iss", "aud", 60);
        let s2 = JwtService::new("b", "iss", "aud", 60);
        let t = s1.issue("u").unwrap();
        assert!(s2.verify(&t).is_err());
    }

    #[test]
    fn issue_with_scopes_roundtrips() {
        let s = JwtService::new("secret", "iss", "aud", 60);
        let scopes = vec!["admin".to_string()];
        let token = s.issue_with_scopes("user-9", &scopes).unwrap();
        let claims = s.verify(&token).unwrap();
        assert_eq!(claims.sub, "user-9");
        assert_eq!(claims.scopes, vec!["admin".to_string()]);
    }

    #[test]
    fn issue_has_empty_scopes_by_default() {
        let s = JwtService::new("secret", "iss", "aud", 60);
        let claims = s.verify(&s.issue("u").unwrap()).unwrap();
        assert!(claims.scopes.is_empty());
    }
}
