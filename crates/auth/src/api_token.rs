//! API token (long-lived bearer token for scripts).

use rand::distributions::{Alphanumeric, DistString};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use uuid::Uuid;

/// API token record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiToken {
    /// Stable token ID.
    pub id: Uuid,
    /// Owner user ID.
    pub user_id: Uuid,
    /// Token name (user-supplied label).
    pub name: String,
    /// Last-4 of the plain token (for display).
    pub last_four: String,
    /// SHA-256 hash of the plain token (hex).
    pub hash: String,
    /// Creation timestamp.
    pub created_at: time::OffsetDateTime,
    /// Optional revocation timestamp.
    pub revoked_at: Option<time::OffsetDateTime>,
}

/// API-token errors.
#[derive(Debug, Error)]
pub enum ApiTokenError {
    /// Token not found.
    #[error("not found")]
    NotFound,
    /// Token revoked.
    #[error("revoked")]
    Revoked,
    /// Hash mismatch.
    #[error("invalid token")]
    Invalid,
}

/// API-token service.
#[derive(Debug, Clone, Default)]
pub struct ApiTokenService;

impl ApiTokenService {
    /// Creates a new service.
    pub const fn new() -> Self {
        Self
    }

    /// Mints a new token for the given user.
    pub fn mint(&self, user_id: Uuid, name: impl Into<String>, prefix: &str) -> (ApiToken, String) {
        let raw = format!(
            "{prefix}_{}",
            Alphanumeric.sample_string(&mut rand::thread_rng(), 40)
        );
        let hash = hex_sha256(&raw);
        let last_four = raw
            .chars()
            .rev()
            .take(4)
            .collect::<String>()
            .chars()
            .rev()
            .collect();
        let token = ApiToken {
            id: Uuid::now_v7(),
            user_id,
            name: name.into(),
            last_four,
            hash,
            created_at: time::OffsetDateTime::now_utc(),
            revoked_at: None,
        };
        (token, raw)
    }

    /// Returns whether `raw` matches the stored hash.
    pub fn matches(raw: &str, token: &ApiToken) -> bool {
        hex_sha256(raw) == token.hash
    }

    /// Marks the token as revoked (returns a new value; original is untouched).
    pub fn revoke(mut token: ApiToken) -> ApiToken {
        token.revoked_at = Some(time::OffsetDateTime::now_utc());
        token
    }
}

fn hex_sha256(s: &str) -> String {
    let mut h = Sha256::new();
    h.update(s.as_bytes());
    let digest = h.finalize();
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mint_then_match_succeeds() {
        let s = ApiTokenService::new();
        let user = Uuid::now_v7();
        let (tok, raw) = s.mint(user, "ci-deploy", "pic");
        assert!(ApiTokenService::matches(&raw, &tok));
        assert!(!ApiTokenService::matches("wrong", &tok));
    }

    #[test]
    fn revoke_sets_timestamp() {
        let s = ApiTokenService::new();
        let (tok, _) = s.mint(Uuid::now_v7(), "x", "pic");
        let r = ApiTokenService::revoke(tok);
        assert!(r.revoked_at.is_some());
    }
}
