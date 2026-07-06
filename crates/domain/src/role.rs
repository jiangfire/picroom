//! Role definitions.

use crate::permission::{Permission, PermissionAction, ResourceType};
use serde::{Deserialize, Serialize};

/// Built-in roles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
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

/// Role → permissions mapping.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RolePermissions {
    /// The role.
    pub role: Role,
    /// Effective permissions.
    pub permissions: Vec<Permission>,
}

impl RolePermissions {
    /// Returns the default permissions for `role`.
    pub fn for_role(role: Role) -> Self {
        Self {
            role,
            permissions: role.default_permissions(),
        }
    }
}

impl Role {
    /// Returns the default permission set for this role.
    pub fn default_permissions(self) -> Vec<Permission> {
        use PermissionAction::{Admin, Create, Delete, Read, Update};
        use ResourceType::{Audit, Image, StoragePolicy, System, Team, User};
        match self {
            Self::Viewer => vec![Permission::new(Image, Read)],
            Self::Uploader => vec![Permission::new(Image, Read), Permission::new(Image, Create)],
            Self::Manager => vec![
                Permission::new(Image, Read),
                Permission::new(Image, Create),
                Permission::new(Image, Update),
                Permission::new(Image, Delete),
                Permission::new(Team, Read),
                Permission::new(Team, Update),
            ],
            Self::Admin => vec![
                Permission::new(Image, Read),
                Permission::new(Image, Create),
                Permission::new(Image, Update),
                Permission::new(Image, Delete),
                Permission::new(Team, Read),
                Permission::new(Team, Update),
                Permission::new(Team, Delete),
                Permission::new(User, Admin),
                Permission::new(Audit, Read),
                Permission::new(StoragePolicy, Admin),
                Permission::new(System, Admin),
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn viewer_has_only_read_image() {
        let perms = Role::Viewer.default_permissions();
        assert_eq!(perms.len(), 1);
        assert_eq!(perms[0].resource, ResourceType::Image);
        assert_eq!(perms[0].action, PermissionAction::Read);
    }

    #[test]
    fn admin_includes_system_admin() {
        let perms = Role::Admin.default_permissions();
        assert!(perms
            .iter()
            .any(|p| p.resource == ResourceType::System && p.action == PermissionAction::Admin));
    }
}
