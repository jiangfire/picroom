//! Permission service.

use crate::ServiceError;
use picroom_auth::{Decision, PermissionAction, RbacEngine, ResourceType, Role};

/// Permission service — thin wrapper over RBAC.
#[derive(Debug, Default, Clone)]
pub struct PermissionService {
    engine: RbacEngine,
}

impl PermissionService {
    /// Creates a new permission service.
    pub const fn new() -> Self {
        Self {
            engine: RbacEngine::new(),
        }
    }

    /// Checks whether any of `roles` may perform `action` on `resource`.
    pub fn check(
        &self,
        roles: &[Role],
        resource: ResourceType,
        action: PermissionAction,
    ) -> Result<(), ServiceError> {
        let decision = self
            .engine
            .check(roles, picroom_auth::Permission::new(resource, action));
        match decision {
            Decision::Allow => Ok(()),
            Decision::Deny => Err(ServiceError::PermissionDenied),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn admin_can_admin() {
        let s = PermissionService::new();
        assert!(s
            .check(
                &[Role::Admin],
                ResourceType::System,
                PermissionAction::Admin
            )
            .is_ok());
    }

    #[test]
    fn viewer_cannot_create() {
        let s = PermissionService::new();
        assert!(s
            .check(
                &[Role::Viewer],
                ResourceType::Image,
                PermissionAction::Create
            )
            .is_err());
    }
}
