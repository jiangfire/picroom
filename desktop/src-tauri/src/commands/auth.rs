// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Picroom Contributors

//! Tauri commands for Picroom admin authentication and session management.

use tauri::AppHandle;

use crate::error::Result;
use crate::state::{perform_login, LoginPayload, LoginResult, Session};
use crate::store::{
    active_profile_name, load_active_profile, load_profiles, save_profiles, set_active_profile_name,
    Profile,
};

#[tauri::command]
pub async fn login(app: AppHandle, payload: LoginPayload) -> Result<LoginResult> {
    let client = reqwest::Client::new();
    let result = perform_login(
        &payload.server_url,
        &payload.email,
        &payload.password,
        &client,
    )
    .await?;

    // Each distinct server URL gets its own persisted profile, keyed by host,
    // so switching between multiple Picroom servers never clobbers credentials.
    let profile_name = profile_name_for(&result.server_url);
    upsert_profile(&app, &profile_name, &result.server_url, &result.email, &result.token)?;
    set_active_profile_name(&app, Some(&profile_name))?;

    Ok(result)
}

/// Derives a stable profile name from a server URL (its `host[:port]`).
fn profile_name_for(server_url: &str) -> String {
    let trimmed = server_url.trim_end_matches('/');
    let without_scheme = match trimmed.split_once("://") {
        Some((_, rest)) => rest,
        None => trimmed,
    };
    without_scheme
        .split('/')
        .next()
        .unwrap_or(without_scheme)
        .to_string()
}

fn upsert_profile(
    app: &AppHandle,
    name: &str,
    server_url: &str,
    email: &str,
    token: &str,
) -> Result<()> {
    let mut profiles = load_profiles(app)?;
    let mut found = false;
    for profile in &mut profiles {
        if profile.name == name {
            profile.server_url = server_url.to_string();
            profile.email = email.to_string();
            profile.token = Some(token.to_string());
            found = true;
            break;
        }
    }
    if !found {
        profiles.push(Profile {
            name: name.to_string(),
            server_url: server_url.to_string(),
            email: email.to_string(),
            token: Some(token.to_string()),
        });
    }
    save_profiles(app, &profiles)
}

#[tauri::command]
pub fn logout(app: AppHandle) -> Result<()> {
    let mut profiles = load_profiles(&app)?;
    let active = active_profile_name(&app)?;
    if let Some(ref name) = active {
        if let Some(profile) = profiles.iter_mut().find(|p| &p.name == name) {
            profile.token = None;
        }
    }
    save_profiles(&app, &profiles)?;
    set_active_profile_name(&app, None)
}

#[tauri::command]
pub fn get_session(app: AppHandle) -> Result<Option<Session>> {
    let profile = load_active_profile(&app)?;
    Ok(profile.and_then(|p| {
        p.token.map(|token| Session {
            name: p.name,
            server_url: p.server_url,
            email: p.email,
            token,
        })
    }))
}

#[tauri::command]
pub fn list_profiles(app: AppHandle) -> Result<Vec<Profile>> {
    load_profiles(&app)
}

#[tauri::command]
pub fn save_profile(app: AppHandle, profile: Profile) -> Result<()> {
    let mut profiles = load_profiles(&app)?;
    let mut found = false;
    for p in &mut profiles {
        if p.name == profile.name {
            *p = profile.clone();
            found = true;
            break;
        }
    }
    if !found {
        profiles.push(profile);
    }
    save_profiles(&app, &profiles)
}

#[tauri::command]
pub fn set_active_profile(app: AppHandle, name: String) -> Result<()> {
    set_active_profile_name(&app, Some(&name))
}

#[tauri::command]
pub fn remove_profile(app: AppHandle, name: String) -> Result<()> {
    let mut profiles = load_profiles(&app)?;
    profiles.retain(|p| p.name != name);
    save_profiles(&app, &profiles)?;
    if active_profile_name(&app)?.as_ref() == Some(&name) {
        set_active_profile_name(&app, None)?;
    }
    Ok(())
}
