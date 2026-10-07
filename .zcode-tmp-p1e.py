# Temporary patch: images.rs handler — Actor, orphan cleanup, error mapping.
p = 'crates/api/src/handlers/images.rs'
src = open(p, encoding='utf-8').read()

old = '''    let bytes = file_bytes.ok_or_else(|| ApiError::bad_request("missing 'file' field"))?;
    let mime = content_type.unwrap_or_else(|| "application/octet-stream".to_string());

    // Attribute the upload to the authenticated principal (never the dev user).
    let actor = auth.user_id;

    // 1. Validate + persist bytes (no jobs yet).
    let mut image = match state.upload.stage(actor, &mime, bytes).await {
        Ok(i) => i,
        Err(e) => {
            let s = format!("{e}");
            if s.contains("empty")
                || s.contains("exceeds")
                || s.contains("unsupported")
                || s.contains("probe")
            {
                return Err(ApiError::bad_request(s));
            }
            tracing::error!("upload failed: {e}");
            return Err(ApiError::from(e));
        }
    };

    // Associate the upload with a team when one was supplied.
    image.team_id = team_id.map(TeamId);

    // 2. Persist metadata if a repo is configured.
    if let Some(repo) = &state.image_repo {
        if let Err(e) = repo.insert(&image).await {
            tracing::error!("repo insert failed: {e}");
            // Image is in storage; surface a 500.
            return Err(ApiError::internal(format!("insert failed: {e}")));
        }
    }

    // 3. Only after the row is committed, enqueue variant jobs — a worker that
    // claims a job must be able to load its `images` row (R-04).
    state.upload.enqueue_variants(&image).await;'''
new = '''    let bytes = file_bytes.ok_or_else(|| ApiError::bad_request("missing 'file' field"))?;
    let mime = content_type.unwrap_or_else(|| "application/octet-stream".to_string());

    // 1. Validate + persist bytes (no jobs yet). The service layer enforces
    // `Image/Create` — including team-scope validation for the `team_id`
    // field, which is attributed server-side (R-05, R-13, D-7).
    let actor = auth.actor();
    let image = match state.upload.stage(&actor, team_id.map(TeamId), &mime, bytes).await {
        Ok(i) => i,
        Err(e) => {
            let s = format!("{e}");
            if s.contains("empty")
                || s.contains("exceeds")
                || s.contains("unsupported")
                || s.contains("corrupt")
            {
                return Err(ApiError::bad_request(s));
            }
            tracing::error!("upload failed: {e}");
            return Err(ApiError::from(e));
        }
    };

    // 2. Persist metadata if a repo is configured. On failure the stored
    // object is removed — an orphan blob nobody can address must not survive
    // a failed upload (R-19).
    if let Some(repo) = &state.image_repo {
        if let Err(e) = repo.insert(&image).await {
            tracing::error!("repo insert failed: {e}");
            if let Err(del) = state.storage.delete(&image.key).await {
                tracing::error!(key = %image.key.as_str(), error = %del, "orphan cleanup failed");
            }
            return Err(ApiError::internal(format!("insert failed: {e}")));
        }
    }

    // 3. Only after the row is committed, enqueue variant jobs — a worker that
    // claims a job must be able to load its `images` row (R-04).
    state.upload.enqueue_variants(&image).await;'''
assert old in src
src = src.replace(old, new)

# delete handler: service-layer enforcement replaces the route IDOR check
old = '''    let image = repo.get(ImageId(id)).await.map_err(ApiError::from)?;
    // IDOR check: only the owner, or a principal permitted to delete images
    // (manager/admin via RBAC), may delete this image.
    if auth.user_id != image.owner_id
        && state
            .permissions
            .check(&auth.roles, ResourceType::Image, PermissionAction::Delete)
            .is_err()
    {
        return Err(ApiError::forbidden("not allowed"));
    }
    // Route deletion through the unified DeleteService (storage + DB + audit).
    // The already-fetched `image` is passed in so we don't look it up twice.
    match &state.delete_service {
        Some(svc) => svc.delete(image).await.map_err(ApiError::from)?,
        None => {
            // Defensive fallback for environments without a DB-backed service.
            if let Err(e) = state.storage.delete(&image.key).await {
                tracing::warn!("storage delete failed: {e}");
            }
        }
    }
    Ok(StatusCode::NO_CONTENT)'''
new = '''    let image = repo.get(ImageId(id)).await.map_err(ApiError::from)?;
    // Authorization happens inside the DeleteService (owner / ACL / RBAC, in
    // the spec §10.3 order) — the route keeps authentication only (D-7).
    // Without a DB-backed service the route-level gate below is the only
    // defense, so it stays as the dev-mode fallback.
    let authorized = auth.user_id == image.owner_id
        || state
            .permissions
            .check(&auth.roles, ResourceType::Image, PermissionAction::Delete)
            .is_ok();
    match &state.delete_service {
        Some(svc) => svc.delete(&auth.actor(), image).await.map_err(ApiError::from)?,
        None => {
            if !authorized {
                return Err(ApiError::forbidden("not allowed"));
            }
            // Defensive fallback for environments without a DB-backed service.
            if let Err(e) = state.storage.delete(&image.key).await {
                tracing::warn!("storage delete failed: {e}");
            }
        }
    }
    Ok(StatusCode::NO_CONTENT)'''
assert old in src
src = src.replace(old, new)
open(p, 'w', encoding='utf-8', newline='\n').write(src)
print("ok")
