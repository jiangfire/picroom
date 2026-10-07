// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Picroom Contributors

//! Permission enums.

use serde::{Deserialize, Serialize};

/// Resource category.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceType {
    /// Image resource.
    Image,
    /// Team resource.
    Team,
    /// User resource.
    User,
    /// Audit log resource.
    Audit,
    /// Storage policy resource.
    StoragePolicy,
    /// System resource.
    System,
}

/// Action on a resource.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionAction {
    /// Read / list.
    Read,
    /// Create.
    Create,
    /// Update.
    Update,
    /// Delete.
    Delete,
    /// Administer (e.g. role changes).
    Admin,
}

/// A permission tuple: resource + action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Permission {
    /// Resource category.
    pub resource: ResourceType,
    /// Action.
    pub action: PermissionAction,
}

impl ResourceType {
    /// Lower-case string identifier (matches the `resource_acls.resource_type`
    /// column convention used by the repositories).
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Image => "image",
            Self::Team => "team",
            Self::User => "user",
            Self::Audit => "audit",
            Self::StoragePolicy => "storage_policy",
            Self::System => "system",
        }
    }
}

impl PermissionAction {
    /// Lower-case string identifier (matches the `resource_acls.permission`
    /// CHECK constraint).
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Create => "create",
            Self::Update => "update",
            Self::Delete => "delete",
            Self::Admin => "admin",
        }
    }
}

impl Permission {
    /// Constructs a new permission.
    pub const fn new(resource: ResourceType, action: PermissionAction) -> Self {
        Self { resource, action }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resource_type_serde_snake_case() {
        assert_eq!(
            serde_json::to_string(&ResourceType::StoragePolicy).unwrap(),
            "\"storage_policy\""
        );
        assert_eq!(
            serde_json::to_string(&ResourceType::System).unwrap(),
            "\"system\""
        );
        let r: ResourceType = serde_json::from_str("\"audit\"").unwrap();
        assert_eq!(r, ResourceType::Audit);
        let r: ResourceType = serde_json::from_str("\"user\"").unwrap();
        assert_eq!(r, ResourceType::User);
    }

    #[test]
    fn permission_action_serde_snake_case() {
        assert_eq!(
            serde_json::to_string(&PermissionAction::Read).unwrap(),
            "\"read\""
        );
        assert_eq!(
            serde_json::to_string(&PermissionAction::Admin).unwrap(),
            "\"admin\""
        );
        let a: PermissionAction = serde_json::from_str("\"delete\"").unwrap();
        assert_eq!(a, PermissionAction::Delete);
    }

    #[test]
    fn permission_serde_roundtrip() {
        let p = Permission::new(ResourceType::Image, PermissionAction::Delete);
        let s = serde_json::to_string(&p).unwrap();
        let back: Permission = serde_json::from_str(&s).unwrap();
        assert_eq!(p, back);
    }

    #[test]
    fn unknown_variant_is_rejected() {
        assert!(serde_json::from_str::<ResourceType>("\"nope\"").is_err());
        assert!(serde_json::from_str::<PermissionAction>("\"explode\"").is_err());
    }
}
