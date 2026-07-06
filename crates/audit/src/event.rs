//! Audit event types.

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

/// An audit-log entry. Append-only; never mutated or deleted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditEvent {
    /// Stable event id.
    pub id: Uuid,
    /// When the action happened.
    pub timestamp: OffsetDateTime,
    /// User id of the actor (None for system actions).
    pub actor_id: Option<Uuid>,
    /// Human-readable actor identifier (email) for convenience.
    pub actor_label: Option<String>,
    /// Action performed.
    pub action: AuditAction,
    /// Target resource type.
    pub target_type: String,
    /// Target resource id.
    pub target_id: Option<String>,
    /// Client IP (if known).
    pub ip: Option<String>,
    /// Client user-agent (if known).
    pub user_agent: Option<String>,
    /// Free-form metadata.
    #[serde(default)]
    pub metadata: serde_json::Value,
}

/// All known audit action types.
///
/// Use a string for forward compatibility; new variants can be added without
/// a schema migration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditAction {
    /// User logged in.
    Login,
    /// User logged out.
    Logout,
    /// User uploaded an image.
    ImageUpload,
    /// User deleted an image.
    ImageDelete,
    /// User created a team.
    TeamCreate,
    /// User added a team member.
    TeamMemberAdd,
    /// User removed a team member.
    TeamMemberRemove,
    /// Admin created a user.
    UserCreate,
    /// Admin changed a user role.
    UserRoleChange,
    /// Admin created a storage policy.
    StoragePolicyCreate,
    /// Admin changed a storage policy.
    StoragePolicyUpdate,
    /// Permission denied (RBAC denial).
    PermissionDenied,
    /// Catch-all for unmapped actions.
    #[serde(other)]
    Other,
}

impl AuditAction {
    /// Stable string identifier.
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Login => "auth.login",
            Self::Logout => "auth.logout",
            Self::ImageUpload => "image.upload",
            Self::ImageDelete => "image.delete",
            Self::TeamCreate => "team.create",
            Self::TeamMemberAdd => "team.member_add",
            Self::TeamMemberRemove => "team.member_remove",
            Self::UserCreate => "user.create",
            Self::UserRoleChange => "user.role_change",
            Self::StoragePolicyCreate => "storage_policy.create",
            Self::StoragePolicyUpdate => "storage_policy.update",
            Self::PermissionDenied => "permission.denied",
            Self::Other => "other",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn action_as_str_is_stable() {
        assert_eq!(AuditAction::Login.as_str(), "auth.login");
        assert_eq!(AuditAction::ImageUpload.as_str(), "image.upload");
        assert_eq!(AuditAction::Other.as_str(), "other");
    }
}