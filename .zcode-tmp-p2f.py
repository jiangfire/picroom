# Temporary patch: update team list/list_members callers to paginated API.
p = 'crates/api/src/handlers/teams.rs'
src = open(p, encoding='utf-8').read()

src = src.replace(
    'use picroom_domain::{Team, TeamId, UserId};',
    'use picroom_domain::{PageReq, Team, TeamId, UserId};',
)

old = '''    let teams = if can_read_all {
        repo.list().await.map_err(ApiError::from)?
    } else {
        repo.list_for_user(auth.user_id).await.map_err(ApiError::from)?
    };
    let items: Vec<serde_json::Value> = teams
        .iter()
        .map(|t| {
            json!({
                "id": t.id.to_string(),
                "name": t.name,
                "slug": t.slug,
                "description": t.description,
                "storage_policy": t.storage_policy,
                "created_at": t.created_at,
            })
        })
        .collect();
    Ok(Json(json!({ "items": items })))
}'''
new = '''    // R-25: bounded queries — clamp the page like /images.
    let page = PageReq {
        limit: 100,
        cursor: None,
    };
    let teams = if can_read_all {
        repo.list(page).await.map_err(ApiError::from)?
    } else {
        repo.list_for_user(auth.user_id, page)
            .await
            .map_err(ApiError::from)?
    };
    let items: Vec<serde_json::Value> = teams
        .items
        .iter()
        .map(|t| {
            json!({
                "id": t.id.to_string(),
                "name": t.name,
                "slug": t.slug,
                "description": t.description,
                "storage_policy": t.storage_policy,
                "created_at": t.created_at,
            })
        })
        .collect();
    Ok(Json(json!({
        "items": items,
        "has_more": teams.has_more,
        "next_cursor": teams.next_cursor,
    })))
}'''
assert old in src
src = src.replace(old, new)

old = '''    let members = repo
        .list_members(TeamId(id))
        .await
        .map_err(ApiError::from)?;
    let items: Vec<serde_json::Value> = members
        .iter()
        .map(|m| {
            json!({
                "user_id": m.user_id.to_string(),
                "role": m.role,
                "joined_at": m.joined_at,
            })
        })
        .collect();
    Ok(Json(json!({ "items": items })))'''
new = '''    let members = repo
        .list_members(
            TeamId(id),
            PageReq {
                limit: 500,
                cursor: None,
            },
        )
        .await
        .map_err(ApiError::from)?;
    let items: Vec<serde_json::Value> = members
        .items
        .iter()
        .map(|m| {
            json!({
                "user_id": m.user_id.to_string(),
                "role": m.role,
                "joined_at": m.joined_at,
            })
        })
        .collect();
    Ok(Json(json!({
        "items": items,
        "has_more": members.has_more,
        "next_cursor": members.next_cursor,
    })))'''
assert old in src
src = src.replace(old, new)
open(p, 'w', encoding='utf-8', newline='\n').write(src)

# in-memory repos: p1_authz.rs MemTeams
p = 'crates/api/tests/p1_authz.rs'
src = open(p, encoding='utf-8').read()
src = src.replace('''    async fn list(&self) -> Result<Vec<Team>, ServiceError> {
        Ok(self.teams.lock().expect("mutex poisoned").clone())
    }
    async fn list_for_user(&self, user_id: UserId) -> Result<Vec<Team>, ServiceError> {
        let members = self.members.lock().expect("mutex poisoned");
        let teams = self.teams.lock().expect("mutex poisoned");
        Ok(teams
            .iter()
            .filter(|t| members.iter().any(|(tid, uid, _)| *tid == t.id.0 && *uid == user_id.0))
            .cloned()
            .collect())
    }''', '''    async fn list(&self, page: PageReq) -> Result<Page<Team>, ServiceError> {
        let all = self.teams.lock().expect("mutex poisoned").clone();
        let limit = page.limit as usize;
        let has_more = all.len() > limit;
        let items: Vec<Team> = all.into_iter().take(limit).collect();
        Ok(Page::new(items, None, page))
    }
    async fn list_for_user(
        &self,
        user_id: UserId,
        page: PageReq,
    ) -> Result<Page<Team>, ServiceError> {
        let members = self.members.lock().expect("mutex poisoned");
        let teams = self.teams.lock().expect("mutex poisoned");
        let all: Vec<Team> = teams
            .iter()
            .filter(|t| members.iter().any(|(tid, uid, _)| *tid == t.id.0 && *uid == user_id.0))
            .cloned()
            .collect();
        let limit = page.limit as usize;
        let has_more = all.len() > limit;
        let items: Vec<Team> = all.into_iter().take(limit).collect();
        let _ = has_more;
        Ok(Page::new(items, None, page))
    }''')
src = src.replace('''    async fn list_members(&self, team_id: TeamId) -> Result<Vec<TeamMember>, ServiceError> {
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
    }''', '''    async fn list_members(
        &self,
        team_id: TeamId,
        page: PageReq,
    ) -> Result<Page<TeamMember>, ServiceError> {
        let members = self.members.lock().expect("mutex poisoned");
        let all: Vec<TeamMember> = members
            .iter()
            .filter(|(tid, _, _)| *tid == team_id.0)
            .map(|(tid, uid, role)| TeamMember {
                team_id: TeamId(*tid),
                user_id: UserId(*uid),
                role: role.clone(),
                joined_at: time::OffsetDateTime::UNIX_EPOCH,
            })
            .collect();
        let limit = page.limit as usize;
        let items: Vec<TeamMember> = all.into_iter().take(limit).collect();
        Ok(Page::new(items, None, page))
    }''')
open(p, 'w', encoding='utf-8', newline='\n').write(src)

# authz.rs MemTeams
p = 'crates/service/src/authz.rs'
src = open(p, encoding='utf-8').read()
src = src.replace('''        async fn list(&self) -> Result<Vec<picroom_domain::Team>, ServiceError> {
            Ok(vec![])
        }
        async fn list_for_user(
            &self,
            _user_id: picroom_domain::UserId,
        ) -> Result<Vec<picroom_domain::Team>, ServiceError> {
            Ok(vec![])
        }''', '''        async fn list(
            &self,
            _page: picroom_domain::PageReq,
        ) -> Result<picroom_domain::Page<picroom_domain::Team>, ServiceError> {
            Ok(picroom_domain::Page::new(vec![], None, picroom_domain::PageReq::default()))
        }
        async fn list_for_user(
            &self,
            _user_id: picroom_domain::UserId,
            _page: picroom_domain::PageReq,
        ) -> Result<picroom_domain::Page<picroom_domain::Team>, ServiceError> {
            Ok(picroom_domain::Page::new(vec![], None, picroom_domain::PageReq::default()))
        }''')
src = src.replace('''        async fn list_members(
            &self,
            _team_id: picroom_domain::TeamId,
        ) -> Result<Vec<picroom_domain::TeamMember>, ServiceError> {
            Ok(vec![])
        }''', '''        async fn list_members(
            &self,
            _team_id: picroom_domain::TeamId,
            _page: picroom_domain::PageReq,
        ) -> Result<picroom_domain::Page<picroom_domain::TeamMember>, ServiceError> {
            Ok(picroom_domain::Page::new(vec![], None, picroom_domain::PageReq::default()))
        }''')
open(p, 'w', encoding='utf-8', newline='\n').write(src)

# api.rs InMemoryTeamRepo
p = 'crates/api/tests/api.rs'
src = open(p, encoding='utf-8').read()
old = '''    async fn list(&self) -> Result<Vec<Team>, ServiceError> {
        Ok(self.teams.clone())
    }'''
if old not in src:
    # find actual signature
    import re
    m = re.search(r'async fn list\(&self\) -> Result<Vec<Team>, ServiceError> \{[\s\S]*?\n    \}', src)
    print("list sig:", m.group(0) if m else "NOT FOUND")
else:
    src = src.replace(old, '''    async fn list(&self, page: PageReq) -> Result<Page<Team>, ServiceError> {
        let limit = page.limit as usize;
        let has_more = self.teams.len() > limit;
        let items: Vec<Team> = self.teams.iter().take(limit).cloned().collect();
        let _ = has_more;
        Ok(Page::new(items, None, page))
    }''')
old2 = '''    async fn list_for_user(&self, user_id: UserId) -> Result<Vec<Team>, ServiceError> {
        let member_team_ids: Vec<TeamId> = self
            .members
            .iter()
            .filter(|m| m.user_id == user_id)
            .map(|m| m.team_id)
            .collect();
        Ok(self
            .teams
            .iter()
            .filter(|t| member_team_ids.contains(&t.id))
            .cloned()
            .collect())
    }'''
if old2 in src:
    src = src.replace(old2, '''    async fn list_for_user(
        &self,
        user_id: UserId,
        page: PageReq,
    ) -> Result<Page<Team>, ServiceError> {
        let member_team_ids: Vec<TeamId> = self
            .members
            .iter()
            .filter(|m| m.user_id == user_id)
            .map(|m| m.team_id)
            .collect();
        let matched: Vec<Team> = self
            .teams
            .iter()
            .filter(|t| member_team_ids.contains(&t.id))
            .cloned()
            .collect();
        let limit = page.limit as usize;
        let items: Vec<Team> = matched.into_iter().take(limit).collect();
        Ok(Page::new(items, None, page))
    }''')
old3 = '''    async fn list_members(&self, team_id: TeamId) -> Result<Vec<TeamMember>, ServiceError> {
        Ok(self
            .members
            .iter()
            .filter(|m| m.team_id == team_id)
            .cloned()
            .collect())
    }'''
if old3 in src:
    src = src.replace(old3, '''    async fn list_members(
        &self,
        team_id: TeamId,
        page: PageReq,
    ) -> Result<Page<TeamMember>, ServiceError> {
        let matched: Vec<TeamMember> = self
            .members
            .iter()
            .filter(|m| m.team_id == team_id)
            .cloned()
            .collect();
        let limit = page.limit as usize;
        let items: Vec<TeamMember> = matched.into_iter().take(limit).collect();
        Ok(Page::new(items, None, page))
    }''')
open(p, 'w', encoding='utf-8', newline='\n').write(src)
print("done")
