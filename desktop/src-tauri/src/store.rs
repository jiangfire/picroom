// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Picroom Contributors

//! Store-backed configuration helpers for the Picroom admin client.

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Runtime};
use tauri_plugin_store::StoreExt;

use crate::error::{Error, Result};

const STORE_NAME: &str = "settings.json";
const PROFILES_KEY: &str = "profiles";
const ACTIVE_KEY: &str = "active_profile";

/// A saved server profile.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    /// Display name (e.g. "prod").
    pub name: String,
    /// Picroom server base URL (e.g. `http://localhost:8080`).
    pub server_url: String,
    /// Login email.
    pub email: String,
    /// JWT access token, when logged in.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
}

/// Loads the saved profiles list, or an empty vector if none exists.
pub fn load_profiles<R: Runtime>(app: &AppHandle<R>) -> Result<Vec<Profile>> {
    let store = app.store(STORE_NAME).map_err(|e| Error(e.to_string()))?;
    let profiles = store
        .get(PROFILES_KEY)
        .and_then(|v| serde_json::from_value::<Vec<Profile>>(v.clone()).ok())
        .unwrap_or_default();
    Ok(profiles)
}

/// Saves the entire profile list to disk.
pub fn save_profiles<R: Runtime>(app: &AppHandle<R>, profiles: &[Profile]) -> Result<()> {
    let store = app.store(STORE_NAME).map_err(|e| Error(e.to_string()))?;
    let value = serde_json::to_value(profiles).map_err(|e| Error(e.to_string()))?;
    store.set(PROFILES_KEY, value);
    store.save().map_err(|e| Error(e.to_string()))
}

/// Returns the active profile name, if any.
pub fn active_profile_name<R: Runtime>(app: &AppHandle<R>) -> Result<Option<String>> {
    let store = app.store(STORE_NAME).map_err(|e| Error(e.to_string()))?;
    Ok(store
        .get(ACTIVE_KEY)
        .and_then(|v| v.as_str().map(String::from)))
}

/// Sets the active profile name.
pub fn set_active_profile_name<R: Runtime>(
    app: &AppHandle<R>,
    name: Option<&str>,
) -> Result<()> {
    let store = app.store(STORE_NAME).map_err(|e| Error(e.to_string()))?;
    store.set(ACTIVE_KEY, serde_json::json!(name));
    store.save().map_err(|e| Error(e.to_string()))
}

/// Returns the currently active profile, if any.
pub fn load_active_profile<R: Runtime>(app: &AppHandle<R>) -> Result<Option<Profile>> {
    let profiles = load_profiles(app)?;
    let active = active_profile_name(app)?;
    Ok(active.and_then(|name| profiles.into_iter().find(|p| p.name == name)))
}
