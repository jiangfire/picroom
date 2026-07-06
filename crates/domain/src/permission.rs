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

impl Permission {
    /// Constructs a new permission.
    pub const fn new(resource: ResourceType, action: PermissionAction) -> Self {
        Self { resource, action }
    }
}
