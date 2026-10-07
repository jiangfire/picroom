// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Picroom Contributors

//! Role-based access control engine.
//!
//! See ADR-0005 for the full model. The canonical permission vocabulary
//! ([`Permission`], [`ResourceType`], [`PermissionAction`]) lives in
//! `picroom-domain` so the service layer and this engine share one definition;
//! it is re-exported here for the one deprecation cycle (R-32).

use serde::{Deserialize, Serialize};

pub use picroom_domain::permission::{Permission, PermissionAction, ResourceType};

/// Permission verdict.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Decision {
    /// Action permitted.
    Allow,
    /// Action denied.
    Deny,
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

impl std::str::FromStr for Role {
    type Err = RoleParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "viewer" => Ok(Self::Viewer),
            "uploader" => Ok(Self::Uploader),
            "manager" => Ok(Self::Manager),
            "admin" => Ok(Self::Admin),
            _ => Err(RoleParseError(s.to_string())),
        }
    }
}

/// Error returned when a role string cannot be parsed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoleParseError(pub String);

impl std::fmt::Display for RoleParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "unknown role: {}", self.0)
    }
}

impl std::error::Error for RoleParseError {}

/// Resource being acted upon (with explicit ACL overrides).
#[derive(Debug, Clone)]
pub struct Resource {
    /// Resource type.
    pub resource_type: ResourceType,
    /// Resource ID (UUID).
    pub id: uuid::Uuid,
    /// Optional owner user ID.
    pub owner_id: Option<uuid::Uuid>,
    /// Team the resource is scoped to, if any. A `Team` resource is its own
    /// scope; a team-shared image carries the owning team here.
    pub team_id: Option<uuid::Uuid>,
}

impl Resource {
    /// Builds a resource description.
    pub const fn new(
        resource_type: ResourceType,
        id: uuid::Uuid,
        owner_id: Option<uuid::Uuid>,
        team_id: Option<uuid::Uuid>,
    ) -> Self {
        Self {
            resource_type,
            id,
            owner_id,
            team_id,
        }
    }

    /// The team whose membership role applies to this resource: for a `Team`
    /// resource the team itself, otherwise the resource's team scope.
    fn team_scope(&self) -> Option<uuid::Uuid> {
        match self.resource_type {
            ResourceType::Team => Some(self.id),
            _ => self.team_id,
        }
    }
}

/// The principal requesting access.
#[derive(Debug, Clone, Default)]
pub struct Actor {
    /// Authenticated user id.
    pub user_id: uuid::Uuid,
    /// Global roles (from the JWT `scopes`).
    pub roles: Vec<Role>,
    /// Role the actor holds within each team they belong to.
    pub team_roles: std::collections::HashMap<uuid::Uuid, Role>,
}

impl Actor {
    /// Builds an actor with only global roles (no team memberships).
    // Not `const` on this toolchain: `HashMap::new` is not yet const-stable
    // here, whatever clippy's `missing_const_for_fn` suggests.
    #[allow(clippy::missing_const_for_fn)]
    pub fn with_roles(user_id: uuid::Uuid, roles: Vec<Role>) -> Self {
        Self {
            user_id,
            roles,
            team_roles: std::collections::HashMap::new(),
        }
    }
}

/// ACL grant subject (mirrors `resource_acls.subject_type`/`subject_id`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AclSubject {
    /// A single user.
    User(uuid::Uuid),
    /// Every member of a team.
    Team(uuid::Uuid),
}

/// ACL grant effect (mirrors `resource_acls.effect`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AclEffect {
    /// Grants the action.
    Allow,
    /// Explicitly forbids the action — the highest-priority rule.
    Deny,
}

/// A loaded ACL grant, reduced to what the engine evaluates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AclRule {
    /// Who the grant applies to.
    pub subject: AclSubject,
    /// Granted action; [`PermissionAction::Admin`] acts as a wildcard.
    pub action: PermissionAction,
    /// Grant effect.
    pub effect: AclEffect,
}

/// RBAC engine — pure-function permission evaluation.
#[derive(Debug, Default, Clone)]
pub struct RbacEngine;

impl RbacEngine {
    /// Creates a new engine.
    pub const fn new() -> Self {
        Self
    }

    /// Evaluates a permission request against a resource and its loaded ACL
    /// grants, in the order mandated by `spec.md` §10.3:
    ///
    /// 1. any matching explicit **deny** (beats admin, owner, everything);
    /// 2. the resource **owner**;
    /// 3. the actor's **team membership role** for the resource's team scope;
    /// 4. a matching **ACL allow** grant (`Admin` permission is a wildcard);
    /// 5. the actor's **global role** default permissions;
    /// 6. default **deny**.
    ///
    /// Note there is deliberately no `Admin ⇒ allow` short-circuit: an explicit
    /// deny row must override the admin role.
    pub fn check(
        &self,
        actor: &Actor,
        resource: &Resource,
        action: PermissionAction,
        grants: &[AclRule],
    ) -> Decision {
        let wanted = Permission::new(resource.resource_type, action);

        // Rule 1 — explicit deny.
        if grants.iter().any(|g| {
            g.effect == AclEffect::Deny
                && Self::rule_applies(g, actor)
                && Self::rule_covers(g, action)
        }) {
            return Decision::Deny;
        }
        // Rule 2 — owner.
        if resource.owner_id == Some(actor.user_id) {
            return Decision::Allow;
        }
        // Rule 3 — team membership role.
        if let Some(team_id) = resource.team_scope() {
            if let Some(role) = actor.team_roles.get(&team_id) {
                if role.default_permissions().contains(&wanted) {
                    return Decision::Allow;
                }
            }
        }
        // Rule 4 — ACL allow grant.
        if grants.iter().any(|g| {
            g.effect == AclEffect::Allow
                && Self::rule_applies(g, actor)
                && Self::rule_covers(g, action)
        }) {
            return Decision::Allow;
        }
        // Rule 5 — global role defaults (admin's blanket grant lives here).
        // A resource that lives *inside* a team is governed by membership
        // (rules 3/4): global `uploader`/`manager` roles must not reach into
        // another team's scope — that was the R-13 hole. Only the admin
        // blanket grant crosses team boundaries, and rule 1 still beats it.
        let membership_scoped =
            resource.resource_type != ResourceType::Team && resource.team_id.is_some();
        let global_ok = actor.roles.iter().any(|r| r == &Role::Admin)
            || (!membership_scoped
                && actor
                    .roles
                    .iter()
                    .flat_map(|r| r.default_permissions())
                    .any(|p| p == wanted));
        if global_ok {
            return Decision::Allow;
        }
        // Rule 6 — default deny.
        Decision::Deny
    }

    /// Evaluates a permission request from global roles only — the pre-ACL
    /// signature kept for route-level checks. No deny rows and no resource
    /// context are involved; prefer [`Self::check`] wherever a resource and
    /// its grants are available.
    pub fn check_roles(&self, roles: &[Role], action: Permission) -> Decision {
        let actor = Actor::with_roles(uuid::Uuid::nil(), roles.to_vec());
        let resource = Resource::new(action.resource, uuid::Uuid::nil(), None, None);
        self.check(&actor, &resource, action.action, &[])
    }

    /// Does `rule`'s subject include `actor`?
    fn rule_applies(rule: &AclRule, actor: &Actor) -> bool {
        match rule.subject {
            AclSubject::User(id) => id == actor.user_id,
            AclSubject::Team(id) => actor.team_roles.contains_key(&id),
        }
    }

    /// Does `rule` cover `action`? An `Admin` grant is a wildcard.
    fn rule_covers(rule: &AclRule, action: PermissionAction) -> bool {
        rule.action == action || rule.action == PermissionAction::Admin
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;

    fn actor(user_id: uuid::Uuid, roles: &[Role]) -> Actor {
        Actor::with_roles(user_id, roles.to_vec())
    }

    fn rule(subject: AclSubject, action: PermissionAction, effect: AclEffect) -> AclRule {
        AclRule {
            subject,
            action,
            effect,
        }
    }

    fn image_resource(owner: Option<uuid::Uuid>, team: Option<uuid::Uuid>) -> Resource {
        Resource::new(ResourceType::Image, uuid::Uuid::now_v7(), owner, team)
    }

    // --- global-role baseline (check_roles) ---

    #[test]
    fn admin_can_do_anything() {
        let e = RbacEngine::new();
        assert_eq!(
            e.check_roles(
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
            e.check_roles(
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
            e.check_roles(
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
            e.check_roles(
                &[Role::Manager],
                Permission::new(ResourceType::System, PermissionAction::Admin)
            ),
            Decision::Deny
        );
    }

    // --- spec 10.3 evaluation order (check) ---

    #[test]
    fn explicit_deny_beats_admin_role() {
        let e = RbacEngine::new();
        let user = uuid::Uuid::now_v7();
        let grants = [rule(
            AclSubject::User(user),
            PermissionAction::Delete,
            AclEffect::Deny,
        )];
        assert_eq!(
            e.check(
                &actor(user, &[Role::Admin]),
                &image_resource(None, None),
                PermissionAction::Delete,
                &grants
            ),
            Decision::Deny,
            "an explicit deny must override the admin role"
        );
    }

    #[test]
    fn explicit_deny_beats_manager_team_role() {
        let e = RbacEngine::new();
        let user = uuid::Uuid::now_v7();
        let team = uuid::Uuid::now_v7();
        let mut a = actor(user, &[]);
        a.team_roles.insert(team, Role::Manager);
        let grants = [rule(
            AclSubject::Team(team),
            PermissionAction::Update,
            AclEffect::Deny,
        )];
        let res = Resource::new(ResourceType::Team, team, None, None);
        assert_eq!(
            e.check(&a, &res, PermissionAction::Update, &grants),
            Decision::Deny
        );
    }

    #[test]
    fn allow_grant_grants_access_with_no_role() {
        let e = RbacEngine::new();
        let user = uuid::Uuid::now_v7();
        let owner = uuid::Uuid::now_v7();
        let grants = [rule(
            AclSubject::User(user),
            PermissionAction::Read,
            AclEffect::Allow,
        )];
        assert_eq!(
            e.check(
                &actor(user, &[]),
                &image_resource(Some(owner), None),
                PermissionAction::Read,
                &grants
            ),
            Decision::Allow
        );
    }

    #[test]
    fn no_grant_no_role_is_denied() {
        let e = RbacEngine::new();
        let owner = uuid::Uuid::now_v7();
        assert_eq!(
            e.check(
                &actor(uuid::Uuid::now_v7(), &[]),
                &image_resource(Some(owner), None),
                PermissionAction::Read,
                &[]
            ),
            Decision::Deny
        );
    }

    #[test]
    fn admin_without_deny_is_still_allowed() {
        let e = RbacEngine::new();
        let owner = uuid::Uuid::now_v7();
        assert_eq!(
            e.check(
                &actor(uuid::Uuid::now_v7(), &[Role::Admin]),
                &image_resource(Some(owner), None),
                PermissionAction::Delete,
                &[]
            ),
            Decision::Allow
        );
    }

    #[test]
    fn owner_is_allowed_for_their_own_resource() {
        let e = RbacEngine::new();
        let owner = uuid::Uuid::now_v7();
        // No role at all — ownership alone grants every action on the image.
        for action in [
            PermissionAction::Read,
            PermissionAction::Update,
            PermissionAction::Delete,
        ] {
            assert_eq!(
                e.check(
                    &actor(owner, &[]),
                    &image_resource(Some(owner), None),
                    action,
                    &[]
                ),
                Decision::Allow,
                "owner must be allowed {action:?}"
            );
        }
        // ...but a deny row still beats ownership.
        let grants = [rule(
            AclSubject::User(owner),
            PermissionAction::Delete,
            AclEffect::Deny,
        )];
        assert_eq!(
            e.check(
                &actor(owner, &[]),
                &image_resource(Some(owner), None),
                PermissionAction::Delete,
                &grants
            ),
            Decision::Deny
        );
    }

    #[test]
    fn team_member_role_grants_image_actions() {
        let e = RbacEngine::new();
        let user = uuid::Uuid::now_v7();
        let team = uuid::Uuid::now_v7();
        let mut uploader = actor(user, &[]);
        uploader.team_roles.insert(team, Role::Uploader);
        // Team uploader may create images in the team scope...
        assert_eq!(
            e.check(
                &uploader,
                &image_resource(None, Some(team)),
                PermissionAction::Create,
                &[]
            ),
            Decision::Allow
        );
        // ...but not delete them (uploader has no Image/Delete).
        assert_eq!(
            e.check(
                &uploader,
                &image_resource(None, Some(team)),
                PermissionAction::Delete,
                &[]
            ),
            Decision::Deny
        );
    }

    #[test]
    fn team_subject_grant_applies_to_members_only() {
        let e = RbacEngine::new();
        let team = uuid::Uuid::now_v7();
        let member = uuid::Uuid::now_v7();
        let outsider = uuid::Uuid::now_v7();
        let mut m = actor(member, &[]);
        m.team_roles.insert(team, Role::Viewer);
        let grants = [rule(
            AclSubject::Team(team),
            PermissionAction::Read,
            AclEffect::Allow,
        )];
        assert_eq!(
            e.check(
                &m,
                &image_resource(None, Some(team)),
                PermissionAction::Read,
                &grants
            ),
            Decision::Allow
        );
        assert_eq!(
            e.check(
                &actor(outsider, &[]),
                &image_resource(None, Some(team)),
                PermissionAction::Read,
                &grants
            ),
            Decision::Deny
        );
    }

    #[test]
    fn global_uploader_cannot_upload_into_a_foreign_team() {
        let e = RbacEngine::new();
        let foreign_team = uuid::Uuid::now_v7();
        assert_eq!(
            e.check(
                &actor(uuid::Uuid::now_v7(), &[Role::Uploader]),
                &image_resource(None, Some(foreign_team)),
                PermissionAction::Create,
                &[]
            ),
            Decision::Deny,
            "global roles must not reach into another team's scope (R-13)"
        );
        // Membership fixes it.
        let mut member = actor(uuid::Uuid::now_v7(), &[Role::Uploader]);
        member.team_roles.insert(foreign_team, Role::Uploader);
        assert_eq!(
            e.check(
                &member,
                &image_resource(None, Some(foreign_team)),
                PermissionAction::Create,
                &[]
            ),
            Decision::Allow
        );
        // Admin blanket grant still crosses team boundaries.
        assert_eq!(
            e.check(
                &actor(uuid::Uuid::now_v7(), &[Role::Admin]),
                &image_resource(None, Some(foreign_team)),
                PermissionAction::Create,
                &[]
            ),
            Decision::Allow
        );
    }

    #[test]
    fn admin_grant_is_a_wildcard() {
        let e = RbacEngine::new();
        let user = uuid::Uuid::now_v7();
        let grants = [rule(
            AclSubject::User(user),
            PermissionAction::Admin,
            AclEffect::Allow,
        )];
        assert_eq!(
            e.check(
                &actor(user, &[]),
                &image_resource(None, None),
                PermissionAction::Delete,
                &grants
            ),
            Decision::Allow
        );
    }

    // --- role vocabulary ---

    #[test]
    fn role_as_str_roundtrips() {
        assert_eq!(Role::Viewer.as_str(), "viewer");
        assert_eq!(Role::Uploader.as_str(), "uploader");
        assert_eq!(Role::Manager.as_str(), "manager");
        assert_eq!(Role::Admin.as_str(), "admin");
    }

    #[test]
    fn role_from_str_valid_is_case_insensitive() {
        assert_eq!(Role::from_str("viewer").unwrap(), Role::Viewer);
        assert_eq!(Role::from_str("UPLOADER").unwrap(), Role::Uploader);
        assert_eq!(Role::from_str("Manager").unwrap(), Role::Manager);
        assert_eq!(Role::from_str("admin").unwrap(), Role::Admin);
    }

    #[test]
    fn role_from_str_invalid() {
        assert!(Role::from_str("superuser").is_err());
    }

    #[test]
    fn permission_new_builds_tuple() {
        let p = Permission::new(ResourceType::Image, PermissionAction::Delete);
        assert_eq!(p.resource, ResourceType::Image);
        assert_eq!(p.action, PermissionAction::Delete);
    }

    /// Task 1.1: the canonical permission vocabulary is `picroom-domain`'s;
    /// this crate re-exports it. Both serde directions must agree on the
    /// snake_case wire format.
    #[test]
    fn reexported_permission_types_serde_roundtrip() {
        let p = Permission::new(ResourceType::StoragePolicy, PermissionAction::Admin);
        let s = serde_json::to_string(&p).unwrap();
        let back: Permission = serde_json::from_str(&s).unwrap();
        assert_eq!(p, back);
        assert_eq!(
            serde_json::to_string(&ResourceType::StoragePolicy).unwrap(),
            "\"storage_policy\""
        );
        assert_eq!(
            serde_json::to_string(&PermissionAction::Read).unwrap(),
            "\"read\""
        );
        let r: ResourceType = serde_json::from_str("\"team\"").unwrap();
        assert_eq!(r, ResourceType::Team);
    }
}
