// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Picroom Contributors

//! Authorization coordinator.
//!
//! Loads the inputs the RBAC engine needs — the resource's ACL grants and the
//! actor's team-membership role — once per check, then evaluates in the
//! `spec.md` §10.3 order. Service-layer entry points ([`UploadService`],
//! [`DeleteService`]) call [`AuthzService::authorize`] so authorization holds
//! even when no HTTP route is involved (D-7).

use crate::repo::{ResourceAclRepository, TeamRepository};
use crate::ServiceError;
use picroom_auth::{AclRule, Actor, PermissionAction, RbacEngine, Resource, ResourceType, Role};
use std::sync::Arc;

/// Authorization service — engine + the repositories it needs to load context.
#[derive(Clone)]
pub struct AuthzService {
    engine: RbacEngine,
    acls: Option<Arc<dyn ResourceAclRepository + Send + Sync>>,
    teams: Option<Arc<dyn TeamRepository + Send + Sync>>,
}

impl std::fmt::Debug for AuthzService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuthzService")
            .field("acl_backed", &self.acls.is_some())
            .field("team_backed", &self.teams.is_some())
            .finish()
    }
}

impl AuthzService {
    /// Creates a service backed by the ACL and team repositories.
    pub fn new(
        acls: Arc<dyn ResourceAclRepository + Send + Sync>,
        teams: Arc<dyn TeamRepository + Send + Sync>,
    ) -> Self {
        Self {
            engine: RbacEngine::new(),
            acls: Some(acls),
            teams: Some(teams),
        }
    }

    /// Creates an engine-only service: global roles and ownership still
    /// apply, but no ACL rows or team memberships can be loaded. Used in dev
    /// mode and tests without a database.
    pub fn without_backends() -> Self {
        Self {
            engine: RbacEngine::new(),
            acls: None,
            teams: None,
        }
    }

    /// Evaluates `action` on `resource` for `actor`, loading the resource's
    /// ACL grants and the actor's team role on the way. `Deny` maps to
    /// [`ServiceError::PermissionDenied`].
    pub async fn authorize(
        &self,
        actor: &Actor,
        resource: &Resource,
        action: PermissionAction,
    ) -> Result<(), ServiceError> {
        // Expand the actor with the team-membership role for the resource's
        // team scope (loaded once per check, not per rule).
        let mut full = actor.clone();
        if let Some(team_id) = team_scope(resource) {
            if let Some(teams) = &self.teams {
                if let Some(role) = teams
                    .member_role(
                        picroom_domain::TeamId(team_id),
                        picroom_domain::UserId(actor.user_id),
                    )
                    .await?
                {
                    if let Ok(role) = Role::from_str(&role) {
                        full.team_roles.insert(team_id, role);
                    }
                }
            }
        }

        // Load the resource's grants.
        let mut rules: Vec<AclRule> = Vec::new();
        if let Some(acls) = &self.acls {
            for g in acls
                .list_grants(resource.resource_type.as_str(), resource.id)
                .await?
            {
                rules.push(AclRule {
                    subject: g.subject,
                    action: g.action,
                    effect: g.effect,
                });
            }
        }

        match self.engine.check(&full, resource, action, &rules) {
            picroom_auth::Decision::Allow => Ok(()),
            picroom_auth::Decision::Deny => Err(ServiceError::PermissionDenied),
        }
    }
}

/// The team whose membership role applies — mirrors `Resource::team_scope`.
fn team_scope(resource: &Resource) -> Option<uuid::Uuid> {
    match resource.resource_type {
        ResourceType::Team => Some(resource.id),
        _ => resource.team_id,
    }
}

use std::str::FromStr;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repo::AclGrant;

    /// In-memory ACL backend for tests.
    #[derive(Default)]
    struct MemAcls {
        grants: std::sync::Mutex<Vec<(String, uuid::Uuid, AclGrant)>>,
    }

    #[async_trait::async_trait]
    impl ResourceAclRepository for MemAcls {
        async fn list_grants(
            &self,
            resource_type: &str,
            resource_id: uuid::Uuid,
        ) -> Result<Vec<AclGrant>, ServiceError> {
            let g = self.grants.lock().expect("mutex poisoned");
            Ok(g.iter()
                .filter(|(t, id, _)| t == resource_type && *id == resource_id)
                .map(|(_, _, g)| g.clone())
                .collect())
        }
        async fn replace_grants(
            &self,
            resource_type: &str,
            resource_id: uuid::Uuid,
            grants: &[AclGrant],
        ) -> Result<(), ServiceError> {
            let mut g = self.grants.lock().expect("mutex poisoned");
            g.retain(|(t, id, _)| !(t == resource_type && *id == resource_id));
            for grant in grants {
                g.push((resource_type.to_string(), resource_id, grant.clone()));
            }
            Ok(())
        }
        async fn revoke(
            &self,
            _resource_type: &str,
            _resource_id: uuid::Uuid,
            _subject: picroom_auth::AclSubject,
        ) -> Result<u64, ServiceError> {
            Ok(0)
        }
    }

    fn image(id: uuid::Uuid, owner: uuid::Uuid) -> Resource {
        Resource::new(ResourceType::Image, id, Some(owner), None)
    }

    /// A deny row on the image must deny even a global admin (R-09/D-9).
    #[tokio::test]
    async fn deny_row_beats_admin() {
        let owner = uuid::Uuid::now_v7();
        let admin = uuid::Uuid::now_v7();
        let img = uuid::Uuid::now_v7();
        let acls = Arc::new(MemAcls::default());
        acls.replace_grants(
            "image",
            img,
            &[AclGrant::deny(
                picroom_auth::AclSubject::User(admin),
                picroom_auth::PermissionAction::Delete,
            )],
        )
        .await
        .unwrap();
        let svc = AuthzService::new(acls.clone(), Arc::new(MemTeams));
        let actor = Actor::with_roles(admin, vec![Role::Admin]);
        assert!(svc
            .authorize(
                &actor,
                &image(img, owner),
                picroom_auth::PermissionAction::Delete
            )
            .await
            .is_err());
    }

    /// An allow grant admits a roleless actor (spec §10.3 rule 3).
    #[tokio::test]
    async fn allow_grant_admits_roleless_actor() {
        let owner = uuid::Uuid::now_v7();
        let user = uuid::Uuid::now_v7();
        let img = uuid::Uuid::now_v7();
        let acls = Arc::new(MemAcls::default());
        acls.replace_grants(
            "image",
            img,
            &[AclGrant::allow(
                picroom_auth::AclSubject::User(user),
                picroom_auth::PermissionAction::Read,
            )],
        )
        .await
        .unwrap();
        let svc = AuthzService::new(acls, Arc::new(MemTeams));
        let actor = Actor::with_roles(user, vec![]);
        assert!(svc
            .authorize(
                &actor,
                &image(img, owner),
                picroom_auth::PermissionAction::Read
            )
            .await
            .is_ok());
        assert!(svc
            .authorize(
                &actor,
                &image(img, owner),
                picroom_auth::PermissionAction::Delete
            )
            .await
            .is_err());
    }

    /// Engine-only mode still enforces global roles (R-05 at the service
    /// layer with no database behind it).
    #[tokio::test]
    async fn without_backends_enforces_global_roles() {
        let svc = AuthzService::without_backends();
        let viewer = Actor::with_roles(uuid::Uuid::now_v7(), vec![Role::Viewer]);
        let res = Resource::new(ResourceType::Image, uuid::Uuid::now_v7(), None, None);
        assert!(svc
            .authorize(&viewer, &res, picroom_auth::PermissionAction::Create)
            .await
            .is_err());
        let uploader = Actor::with_roles(uuid::Uuid::now_v7(), vec![Role::Uploader]);
        assert!(svc
            .authorize(&uploader, &res, picroom_auth::PermissionAction::Create)
            .await
            .is_ok());
    }

    /// Empty team backend stub.
    struct MemTeams;

    #[async_trait::async_trait]
    impl TeamRepository for MemTeams {
        async fn create(&self, _team: &picroom_domain::Team) -> Result<(), ServiceError> {
            Ok(())
        }
        async fn get(
            &self,
            _id: picroom_domain::TeamId,
        ) -> Result<picroom_domain::Team, ServiceError> {
            Err(ServiceError::Internal("unused".into()))
        }
        async fn list(&self) -> Result<Vec<picroom_domain::Team>, ServiceError> {
            Ok(vec![])
        }
        async fn list_for_user(
            &self,
            _user_id: picroom_domain::UserId,
        ) -> Result<Vec<picroom_domain::Team>, ServiceError> {
            Ok(vec![])
        }
        async fn add_member(
            &self,
            _team_id: picroom_domain::TeamId,
            _user_id: picroom_domain::UserId,
            _role: &str,
        ) -> Result<(), ServiceError> {
            Ok(())
        }
        async fn list_members(
            &self,
            _team_id: picroom_domain::TeamId,
        ) -> Result<Vec<picroom_domain::TeamMember>, ServiceError> {
            Ok(vec![])
        }
        async fn member_role(
            &self,
            _team_id: picroom_domain::TeamId,
            _user_id: picroom_domain::UserId,
        ) -> Result<Option<String>, ServiceError> {
            Ok(None)
        }
    }
}
