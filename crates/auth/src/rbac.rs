//! Role-based access control engine.
//!
//! See ADR-0005 for the full model.

use serde::{Deserialize, Serialize};

/// Permission verdict.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Decision {
    /// Action permitted.
    Allow,
    /// Action denied.
    Deny,
}

/// Resource category.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
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

impl Permission {
    /// Constructs a new permission.
    pub const fn new(resource: ResourceType, action: PermissionAction) -> Self {
        Self { resource, action }
    }
}

/// Built-in roles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Role {
    /// Read-only.
    Viewer,
    /// Read + create.
    Uploader,
    /// All image + team-management actions.
    Manager,
    /// Everything.
    Admin,
}

impl Role {
    /// Returns the default permission set for this role.
    pub fn default_permissions(self) -> Vec<Permission> {
        match self {
            Self::Viewer => vec![Permission::new(ResourceType::Image, PermissionAction::Read)],
            Self::Uploader => vec![
                Permission::new(ResourceType::Image, PermissionAction::Read),
                Permission::new(ResourceType::Image, PermissionAction::Create),
            ],
            Self::Manager => vec![
                Permission::new(ResourceType::Image, PermissionAction::Read),
                Permission::new(ResourceType::Image, PermissionAction::Create),
                Permission::new(ResourceType::Image, PermissionAction::Update),
                Permission::new(ResourceType::Image, PermissionAction::Delete),
                Permission::new(ResourceType::Team, PermissionAction::Read),
                Permission::new(ResourceType::Team, PermissionAction::Update),
            ],
            Self::Admin => vec![
                Permission::new(ResourceType::Image, PermissionAction::Read),
                Permission::new(ResourceType::Image, PermissionAction::Create),
                Permission::new(ResourceType::Image, PermissionAction::Update),
                Permission::new(ResourceType::Image, PermissionAction::Delete),
                Permission::new(ResourceType::Team, PermissionAction::Read),
                Permission::new(ResourceType::Team, PermissionAction::Update),
                Permission::new(ResourceType::Team, PermissionAction::Delete),
                Permission::new(ResourceType::User, PermissionAction::Admin),
                Permission::new(ResourceType::Audit, PermissionAction::Read),
                Permission::new(ResourceType::StoragePolicy, PermissionAction::Admin),
                Permission::new(ResourceType::System, PermissionAction::Admin),
            ],
        }
    }

    /// Lower-case string identifier (matches the SQL `CHECK` constraint).
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Viewer => "viewer",
            Self::Uploader => "uploader",
            Self::Manager => "manager",
            Self::Admin => "admin",
        }
    }
}

/// Resource being acted upon (with explicit ACL overrides).
#[derive(Debug, Clone)]
pub struct Resource {
    /// Resource type.
    pub resource_type: ResourceType,
    /// Resource ID (UUID).
    pub id: uuid::Uuid,
    /// Optional owner user ID.
    pub owner_id: Option<uuid::Uuid>,
}

/// RBAC engine — pure-function permission evaluation.
#[derive(Debug, Default, Clone)]
pub struct RbacEngine;

impl RbacEngine {
    /// Creates a new engine.
    pub const fn new() -> Self {
        Self
    }

    /// Evaluates a permission request.
    ///
    /// Rules:
    /// 1. Admin role ⇒ Allow everything.
    /// 2. Otherwise check whether any of the actor's roles grants the action.
    pub fn check(&self, roles: &[Role], action: Permission) -> Decision {
        if roles.contains(&Role::Admin) {
            return Decision::Allow;
        }
        let has = roles
            .iter()
            .flat_map(|r| r.default_permissions())
            .any(|p| p == action);
        if has {
            Decision::Allow
        } else {
            Decision::Deny
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn admin_can_do_anything() {
        let e = RbacEngine::new();
        assert_eq!(
            e.check(
                &[Role::Admin],
                Permission::new(ResourceType::System, PermissionAction::Admin)
            ),
            Decision::Allow
        );
    }

    #[test]
    fn viewer_cannot_create() {
        let e = RbacEngine::new();
        assert_eq!(
            e.check(
                &[Role::Viewer],
                Permission::new(ResourceType::Image, PermissionAction::Create)
            ),
            Decision::Deny
        );
    }

    #[test]
    fn uploader_can_create_image() {
        let e = RbacEngine::new();
        assert_eq!(
            e.check(
                &[Role::Uploader],
                Permission::new(ResourceType::Image, PermissionAction::Create)
            ),
            Decision::Allow
        );
    }

    #[test]
    fn manager_cannot_admin_system() {
        let e = RbacEngine::new();
        assert_eq!(
            e.check(
                &[Role::Manager],
                Permission::new(ResourceType::System, PermissionAction::Admin)
            ),
            Decision::Deny
        );
    }
}
