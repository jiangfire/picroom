// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Picroom Contributors

//! P1 remediation integration tests: authorization at the HTTP layer.
//!
//! Covers the R-05 / R-13 / R-08 / D-10 acceptance criteria against in-memory
//! repositories so they run everywhere (the production repositories are the
//! thin SQL wrappers proven by the repo tests).

use axum::body::Body;
use axum::http::{Request, StatusCode};
use bytes::Bytes;
use http_body_util::BodyExt;
use picroom_api::AppState;
use picroom_audit::NoopAuditSink;
use picroom_domain::{Image, ImageId, Page, PageReq, StorageKey, Team, TeamId, TeamMember, UserId};
use picroom_service::repo::{
    AclGrant, ImageRepository, ResourceAclRepository, SessionRepository, SessionRow,
    TeamRepository,
};
use picroom_service::{AuthzService, ServiceError};
use picroom_storage::driver::LocalDriver;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tower::ServiceExt;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// In-memory backends
// ---------------------------------------------------------------------------

#[derive(Default)]
struct MemSessions {
    rows: Mutex<HashMap<Uuid, SessionRow>>,
}

#[async_trait::async_trait]
impl SessionRepository for MemSessions {
    async fn create(&self, session: &SessionRow) -> Result<(), ServiceError> {
        self.rows
            .lock()
            .expect("mutex poisoned")
            .insert(session.id, session.clone());
        Ok(())
    }
    async fn get_active(&self, id: Uuid) -> Result<Option<SessionRow>, ServiceError> {
        Ok(self.rows.lock().expect("mutex poisoned").get(&id).cloned())
    }
    async fn revoke(&self, id: Uuid) -> Result<u64, ServiceError> {
        Ok(self
            .rows
            .lock()
            .expect("mutex poisoned")
            .remove(&id)
            .map(|_| 1)
            .unwrap_or(0))
    }
    async fn revoke_all_for_user(&self, user_id: Uuid) -> Result<u64, ServiceError> {
        let mut rows = self.rows.lock().expect("mutex poisoned");
        let before = rows.len();
        rows.retain(|_, s| s.user_id != user_id);
        Ok((before - rows.len()) as u64)
    }
}

#[derive(Default)]
struct MemTeams {
    teams: Mutex<Vec<Team>>,
    members: Mutex<Vec<(Uuid, Uuid, String)>>, // (team, user, role)
}

#[async_trait::async_trait]
impl TeamRepository for MemTeams {
    async fn create(&self, team: &Team) -> Result<(), ServiceError> {
        self.teams
            .lock()
            .expect("mutex poisoned")
            .push(team.clone());
        Ok(())
    }
    async fn get(&self, id: TeamId) -> Result<Team, ServiceError> {
        self.teams
            .lock()
            .expect("mutex poisoned")
            .iter()
            .find(|t| t.id == id)
            .cloned()
            .ok_or(picroom_domain::DomainError::NotFound.into())
    }
    async fn list(&self) -> Result<Vec<Team>, ServiceError> {
        Ok(self.teams.lock().expect("mutex poisoned").clone())
    }
    async fn list_for_user(&self, user_id: UserId) -> Result<Vec<Team>, ServiceError> {
        let members = self.members.lock().expect("mutex poisoned");
        let teams = self.teams.lock().expect("mutex poisoned");
        Ok(teams
            .iter()
            .filter(|t| {
                members
                    .iter()
                    .any(|(tid, uid, _)| *tid == t.id.0 && *uid == user_id.0)
            })
            .cloned()
            .collect())
    }
    async fn add_member(
        &self,
        team_id: TeamId,
        user_id: UserId,
        role: &str,
    ) -> Result<(), ServiceError> {
        let mut members = self.members.lock().expect("mutex poisoned");
        members.retain(|(tid, uid, _)| !(*tid == team_id.0 && *uid == user_id.0));
        members.push((team_id.0, user_id.0, role.to_string()));
        Ok(())
    }
    async fn list_members(&self, team_id: TeamId) -> Result<Vec<TeamMember>, ServiceError> {
        let members = self.members.lock().expect("mutex poisoned");
        Ok(members
            .iter()
            .filter(|(tid, _, _)| *tid == team_id.0)
            .map(|(tid, uid, role)| TeamMember {
                team_id: TeamId(*tid),
                user_id: UserId(*uid),
                role: role.clone(),
                joined_at: time::OffsetDateTime::UNIX_EPOCH,
            })
            .collect())
    }
    async fn member_role(
        &self,
        team_id: TeamId,
        user_id: UserId,
    ) -> Result<Option<String>, ServiceError> {
        Ok(self
            .members
            .lock()
            .expect("mutex poisoned")
            .iter()
            .find(|(tid, uid, _)| *tid == team_id.0 && *uid == user_id.0)
            .map(|(_, _, role)| role.clone()))
    }
}

#[derive(Default)]
struct MemAcls {
    grants: Mutex<Vec<(String, Uuid, AclGrant)>>,
}

#[async_trait::async_trait]
impl ResourceAclRepository for MemAcls {
    async fn list_grants(
        &self,
        resource_type: &str,
        resource_id: Uuid,
    ) -> Result<Vec<AclGrant>, ServiceError> {
        Ok(self
            .grants
            .lock()
            .expect("mutex poisoned")
            .iter()
            .filter(|(t, id, _)| t == resource_type && *id == resource_id)
            .map(|(_, _, g)| g.clone())
            .collect())
    }
    async fn replace_grants(
        &self,
        resource_type: &str,
        resource_id: Uuid,
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
        resource_type: &str,
        resource_id: Uuid,
        subject: picroom_auth::AclSubject,
    ) -> Result<u64, ServiceError> {
        let mut g = self.grants.lock().expect("mutex poisoned");
        let before = g.len();
        g.retain(|(t, id, grant)| {
            !(t == resource_type && *id == resource_id && grant.subject == subject)
        });
        Ok((before - g.len()) as u64)
    }
}

#[derive(Default)]
struct MemImages {
    rows: Mutex<Vec<Image>>,
}

#[async_trait::async_trait]
impl ImageRepository for MemImages {
    async fn insert(&self, image: &Image) -> Result<(), ServiceError> {
        self.rows
            .lock()
            .expect("mutex poisoned")
            .push(image.clone());
        Ok(())
    }
    async fn get(&self, id: ImageId) -> Result<Image, ServiceError> {
        self.rows
            .lock()
            .expect("mutex poisoned")
            .iter()
            .find(|i| i.id == id)
            .cloned()
            .ok_or(picroom_domain::DomainError::NotFound.into())
    }
    async fn list_for_owner(
        &self,
        _owner_id: Uuid,
        _page: PageReq,
    ) -> Result<Page<Image>, ServiceError> {
        Ok(Page::new(vec![], None, PageReq::default()))
    }
    async fn delete(&self, id: ImageId) -> Result<(), ServiceError> {
        self.rows
            .lock()
            .expect("mutex poisoned")
            .retain(|i| i.id != id);
        Ok(())
    }
    async fn ping(&self) -> Result<(), ServiceError> {
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

fn tempdir() -> std::path::PathBuf {
    let base = std::env::temp_dir().join(format!("picroom-p1-{}", Uuid::now_v7()));
    std::fs::create_dir_all(&base).unwrap();
    base
}

struct TestApp {
    app: axum::Router,
    teams: Arc<MemTeams>,
    sessions: Arc<MemSessions>,
    images: Arc<MemImages>,
    acls: Arc<MemAcls>,
}

/// Builds the app the way the binary does for the pieces under test:
/// upload/delete services carry the AuthzService backed by the same
/// in-memory repos the state exposes.
fn build_p1_app() -> TestApp {
    let storage = Arc::new(LocalDriver::new(tempdir(), "/i"));
    let teams = Arc::new(MemTeams::default());
    let sessions = Arc::new(MemSessions::default());
    let images = Arc::new(MemImages::default());
    let acls = Arc::new(MemAcls::default());

    let authz = Arc::new(AuthzService::new(acls.clone(), teams.clone()));
    let mut state = AppState::for_dev(storage as Arc<_>, Arc::new(NoopAuditSink))
        .with_team_repo(teams.clone() as Arc<_>)
        .with_image_repo(images.clone() as Arc<_>)
        .with_session_repo(sessions.clone() as Arc<_>)
        .with_acl_repo(acls.clone() as Arc<_>)
        .with_authz(authz.clone())
        .with_upload_authz(authz);

    // Route deletes through the service (as api_cmd does) so enforcement
    // happens in the service layer.
    let storage_writer: Arc<dyn picroom_storage::StorageWriter + Send + Sync> =
        Arc::new(picroom_api::StorageWriterFromArc(
            state.storage.clone() as Arc<dyn picroom_storage::Storage>
        ));
    let audit_arc: Arc<dyn picroom_audit::AuditSink> = Arc::new(NoopAuditSink);
    state.delete_service = Some(Arc::new(
        picroom_service::DeleteService::new(storage_writer, images.clone() as Arc<_>, audit_arc)
            .with_authz(state.authz.clone()),
    ));

    let app = picroom_api::build_router(Arc::new(state));
    TestApp {
        app,
        teams,
        sessions,
        images,
        acls,
    }
}

fn token_for(user: Uuid, role: &str, sid: Option<Uuid>) -> String {
    let jwt = picroom_auth::JwtService::new("dev-secret", "picroom", "picroom-api", 3600);
    let scopes = vec![role.to_string()];
    let token = jwt
        .issue_session(user.to_string(), &scopes, sid.map(|s| s.to_string()))
        .unwrap();
    format!("Bearer {token}")
}

async fn send(app: &axum::Router, req: Request<Body>) -> (StatusCode, Value) {
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let json = if body.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&body).unwrap_or(Value::Null)
    };
    (status, json)
}

fn make_png(w: u32, h: u32) -> Bytes {
    use std::io::Cursor;
    let img = image::RgbImage::from_fn(w, h, |x, y| image::Rgb([x as u8, y as u8, 64]));
    let mut buf = Vec::new();
    image::DynamicImage::ImageRgb8(img)
        .write_to(&mut Cursor::new(&mut buf), image::ImageFormat::Png)
        .unwrap();
    Bytes::from(buf)
}

fn multipart_upload(png: &[u8], team_id: Option<Uuid>) -> Request<Body> {
    let boundary = "----picroom-p1-boundary";
    let mut body = Vec::new();
    body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
    body.extend_from_slice(
        b"Content-Disposition: form-data; name=\"file\"; filename=\"t.png\"\r\n",
    );
    body.extend_from_slice(b"Content-Type: image/png\r\n\r\n");
    body.extend_from_slice(png);
    body.extend_from_slice(b"\r\n");
    if let Some(tid) = team_id {
        body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
        body.extend_from_slice(b"Content-Disposition: form-data; name=\"team_id\"\r\n\r\n");
        body.extend_from_slice(tid.to_string().as_bytes());
        body.extend_from_slice(b"\r\n");
    }
    body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
    Request::builder()
        .method("POST")
        .uri("/api/v1/images")
        .header(
            "content-type",
            format!("multipart/form-data; boundary={boundary}"),
        )
        .body(Body::from(body))
        .unwrap()
}

// ---------------------------------------------------------------------------
// R-05: viewer cannot upload
// ---------------------------------------------------------------------------

#[tokio::test]
async fn viewer_upload_is_rejected_with_403() {
    let t = build_p1_app();
    let viewer = token_for(Uuid::now_v7(), "viewer", None);
    let req = {
        let mut r = multipart_upload(&make_png(20, 10), None);
        r.headers_mut()
            .insert("authorization", viewer.parse().unwrap());
        r
    };
    let (status, _) = send(&t.app, req).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "viewer upload must be 403");
}

#[tokio::test]
async fn uploader_upload_is_accepted() {
    let t = build_p1_app();
    let uploader = token_for(Uuid::now_v7(), "uploader", None);
    let req = {
        let mut r = multipart_upload(&make_png(20, 10), None);
        r.headers_mut()
            .insert("authorization", uploader.parse().unwrap());
        r
    };
    let (status, _) = send(&t.app, req).await;
    assert_eq!(status, StatusCode::OK, "uploader upload must be 200");
}

// ---------------------------------------------------------------------------
// R-13: team scoping
// ---------------------------------------------------------------------------

#[tokio::test]
async fn team_list_scopes_to_membership_and_get_hides_foreign_teams() {
    let t = build_p1_app();
    let alice = Uuid::now_v7();
    let bob = Uuid::now_v7();

    let team_a = TeamId(Uuid::now_v7());
    let team_b = TeamId(Uuid::now_v7());
    t.teams
        .create(&Team {
            id: team_a,
            name: "a".into(),
            slug: "team-a".into(),
            description: None,
            storage_policy: None,
            created_at: time::OffsetDateTime::now_utc(),
        })
        .await
        .unwrap();
    t.teams
        .create(&Team {
            id: team_b,
            name: "b".into(),
            slug: "team-b".into(),
            description: None,
            storage_policy: None,
            created_at: time::OffsetDateTime::now_utc(),
        })
        .await
        .unwrap();
    t.teams
        .add_member(team_a, UserId(alice), "uploader")
        .await
        .unwrap();
    t.teams
        .add_member(team_b, UserId(bob), "uploader")
        .await
        .unwrap();

    // Alice lists only her team.
    let alice_token = token_for(alice, "viewer", None);
    let req = Request::builder()
        .method("GET")
        .uri("/api/v1/teams")
        .header("authorization", &alice_token)
        .body(Body::empty())
        .unwrap();
    let (status, body) = send(&t.app, req).await;
    assert_eq!(status, StatusCode::OK);
    let items = body["items"].as_array().unwrap();
    assert_eq!(items.len(), 1, "alice sees only her team");
    assert_eq!(items[0]["id"].as_str().unwrap(), team_a.0.to_string());

    // Alice cannot read Bob's team (404 — existence is not revealed).
    let req = Request::builder()
        .method("GET")
        .uri(format!("/api/v1/teams/{}", team_b.0))
        .header("authorization", &alice_token)
        .body(Body::empty())
        .unwrap();
    let (status, _) = send(&t.app, req).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // Alice cannot read Bob's roster either.
    let req = Request::builder()
        .method("GET")
        .uri(format!("/api/v1/teams/{}/members", team_b.0))
        .header("authorization", &alice_token)
        .body(Body::empty())
        .unwrap();
    let (status, _) = send(&t.app, req).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // Alice (plain viewer globally) can still see her own team by id.
    let req = Request::builder()
        .method("GET")
        .uri(format!("/api/v1/teams/{}", team_a.0))
        .header("authorization", &alice_token)
        .body(Body::empty())
        .unwrap();
    let (status, _) = send(&t.app, req).await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn upload_into_foreign_team_is_rejected() {
    let t = build_p1_app();
    let alice = Uuid::now_v7();
    let foreign = TeamId(Uuid::now_v7());
    t.teams
        .create(&Team {
            id: foreign,
            name: "foreign".into(),
            slug: "foreign".into(),
            description: None,
            storage_policy: None,
            created_at: time::OffsetDateTime::now_utc(),
        })
        .await
        .unwrap();

    let uploader_token = token_for(alice, "uploader", None);
    let req = {
        let mut r = multipart_upload(&make_png(20, 10), Some(foreign.0));
        r.headers_mut()
            .insert("authorization", uploader_token.parse().unwrap());
        r
    };
    let (status, _) = send(&t.app, req).await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "non-member upload into a team must be 403"
    );
}

// ---------------------------------------------------------------------------
// R-08 / R-20 / D-6: sessions
// ---------------------------------------------------------------------------

#[tokio::test]
async fn logout_revokes_the_session_and_the_token_dies() {
    let t = build_p1_app();
    let user = Uuid::now_v7();
    let sid = Uuid::now_v7();
    t.sessions
        .create(&SessionRow {
            id: sid,
            user_id: user,
            expires_at: time::OffsetDateTime::now_utc() + time::Duration::seconds(3600),
        })
        .await
        .unwrap();

    let auth_header = token_for(user, "uploader", Some(sid));

    // The session-bound token works.
    let req = Request::builder()
        .method("GET")
        .uri("/api/v1/teams")
        .header("authorization", &auth_header)
        .body(Body::empty())
        .unwrap();
    let (status, _) = send(&t.app, req).await;
    assert_eq!(status, StatusCode::OK, "live session must authenticate");

    // Logout revokes it.
    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/auth/logout")
        .header("authorization", &auth_header)
        .body(Body::empty())
        .unwrap();
    let (status, _) = send(&t.app, req).await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    // The same token is now dead.
    let req = Request::builder()
        .method("GET")
        .uri("/api/v1/teams")
        .header("authorization", &auth_header)
        .body(Body::empty())
        .unwrap();
    let (status, _) = send(&t.app, req).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "revoked session must 401");
}

#[tokio::test]
async fn sid_token_without_session_row_is_rejected() {
    let t = build_p1_app();
    let user = Uuid::now_v7();
    let sid = Uuid::now_v7(); // never created
    let token = token_for(user, "uploader", Some(sid));
    let req = Request::builder()
        .method("GET")
        .uri("/api/v1/teams")
        .header("authorization", &token)
        .body(Body::empty())
        .unwrap();
    let (status, _) = send(&t.app, req).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

// ---------------------------------------------------------------------------
// D-10: ACL endpoints
// ---------------------------------------------------------------------------

async fn seed_image(t: &TestApp, owner: Uuid) -> Uuid {
    let id = Uuid::now_v7();
    t.images
        .insert(&Image {
            id: ImageId(id),
            owner_id: UserId(owner),
            team_id: None,
            key: StorageKey::parse(&format!("img/{id}.bin")).unwrap(),
            content_type: "image/png".into(),
            bytes: 1,
            width: 1,
            height: 1,
            sha256: None,
            variants: vec![],
            created_at: time::OffsetDateTime::now_utc(),
        })
        .await
        .unwrap();
    id
}

#[tokio::test]
async fn acl_put_then_get_is_idempotent_and_delete_removes() {
    let t = build_p1_app();
    let owner = Uuid::now_v7();
    let img = seed_image(&t, owner).await;
    let grantee = Uuid::now_v7();
    let owner_token = token_for(owner, "viewer", None); // owner via rule 2, no global role

    // PUT the full grant set — replace semantics.
    let body = serde_json::json!({
        "grants": [
            {"subject_type": "user", "subject_id": grantee, "permission": "read", "effect": "allow"},
            {"subject_type": "user", "subject_id": grantee, "permission": "delete", "effect": "deny"}
        ]
    });
    let req = Request::builder()
        .method("PUT")
        .uri(format!("/api/v1/images/{img}/acl"))
        .header("authorization", &owner_token)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    let (status, resp) = send(&t.app, req).await;
    assert_eq!(status, StatusCode::OK, "PUT acl: {resp}");

    // GET shows the same set.
    let req = Request::builder()
        .method("GET")
        .uri(format!("/api/v1/images/{img}/acl"))
        .header("authorization", &owner_token)
        .body(Body::empty())
        .unwrap();
    let (status, resp) = send(&t.app, req).await;
    assert_eq!(status, StatusCode::OK);
    let items = resp["items"].as_array().unwrap();
    assert_eq!(items.len(), 2);

    // PUT twice → still exactly the same set (idempotent replace).
    let req = Request::builder()
        .method("PUT")
        .uri(format!("/api/v1/images/{img}/acl"))
        .header("authorization", &owner_token)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    let (status, _) = send(&t.app, req).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(t.acls.list_grants("image", img).await.unwrap().len(), 2);

    // DELETE removes exactly one grant.
    let req = Request::builder()
        .method("DELETE")
        .uri(format!("/api/v1/images/{img}/acl/user/{grantee}?x=1"))
        .header("authorization", &owner_token)
        .body(Body::empty())
        .unwrap();
    let (status, _) = send(&t.app, req).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let left = t.acls.list_grants("image", img).await.unwrap();
    assert_eq!(left.len(), 0, "DELETE removes all grants of the subject");
}

#[tokio::test]
async fn non_owner_cannot_manage_acl() {
    let t = build_p1_app();
    let owner = Uuid::now_v7();
    let stranger = Uuid::now_v7();
    let img = seed_image(&t, owner).await;

    let stranger_token = token_for(stranger, "uploader", None);
    let req = Request::builder()
        .method("GET")
        .uri(format!("/api/v1/images/{img}/acl"))
        .header("authorization", &stranger_token)
        .body(Body::empty())
        .unwrap();
    let (status, _) = send(&t.app, req).await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    let body = serde_json::json!({"grants": []});
    let req = Request::builder()
        .method("PUT")
        .uri(format!("/api/v1/images/{img}/acl"))
        .header("authorization", &stranger_token)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    let (status, _) = send(&t.app, req).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn deny_grant_blocks_even_admin_delete_via_service() {
    let t = build_p1_app();
    let owner = Uuid::now_v7();
    let admin = Uuid::now_v7();
    let img = seed_image(&t, owner).await;
    let owner_token = token_for(owner, "viewer", None);

    // Owner denies the admin delete on this image.
    let body = serde_json::json!({
        "grants": [
            {"subject_type": "user", "subject_id": admin, "permission": "delete", "effect": "deny"}
        ]
    });
    let req = Request::builder()
        .method("PUT")
        .uri(format!("/api/v1/images/{img}/acl"))
        .header("authorization", &owner_token)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    let (status, _) = send(&t.app, req).await;
    assert_eq!(status, StatusCode::OK);

    // Admin delete → 403 (deny beats the admin role).
    let admin_token = token_for(admin, "admin", None);
    let req = Request::builder()
        .method("DELETE")
        .uri(format!("/api/v1/images/{img}"))
        .header("authorization", &admin_token)
        .body(Body::empty())
        .unwrap();
    let (status, _) = send(&t.app, req).await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "explicit deny must beat admin"
    );

    // Owner delete still works.
    let req = Request::builder()
        .method("DELETE")
        .uri(format!("/api/v1/images/{img}"))
        .header("authorization", &owner_token)
        .body(Body::empty())
        .unwrap();
    let (status, _) = send(&t.app, req).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
}

/// Keep the unused-import lint honest about helpers only used in some tests.
#[allow(unused)]
fn _t(_: Duration) {}
