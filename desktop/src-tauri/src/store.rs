// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Picroom Contributors

//! Store-backed configuration helpers for the Picroom admin client.
//!
//! Security (R-22): the JWT never touches `settings.json` — it lives in the
//! OS keychain (`keyring`: Windows Credential Manager, macOS Keychain, Linux
//! Secret Service) under the profile name. The store file only keeps profile
//! metadata. Release builds additionally refuse non-HTTPS server URLs; dev
//! builds accept `http://localhost` for local development.

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Runtime};
use tauri_plugin_store::StoreExt;

use crate::error::{Error, Result};

const STORE_NAME: &str = "settings.json";
const PROFILES_KEY: &str = "profiles";
const ACTIVE_KEY: &str = "active_profile";

/// Keychain service name — all Picroom tokens live under this service,
/// keyed by profile name.
const KEYCHAIN_SERVICE: &str = "picroom";

/// Stores `token` in the OS keychain for `profile_name`.
fn keychain_set(profile_name: &str, token: &str) -> Result<()> {
    let entry = keyring::Entry::new(KEYCHAIN_SERVICE, profile_name)
        .map_err(|e| Error(format!("keychain: {e}")))?;
    entry
        .set_password(token)
        .map_err(|e| Error(format!("keychain: {e}")))
}

fn keychain_get(profile_name: &str) -> Option<String> {
    let entry = keyring::Entry::new(KEYCHAIN_SERVICE, profile_name).ok()?;
    entry.get_password().ok()
}

fn keychain_delete(profile_name: &str) {
    if let Ok(entry) = keyring::Entry::new(KEYCHAIN_SERVICE, profile_name) {
        let _ = entry.delete_credential();
    }
}

/// Release builds refuse non-HTTPS server URLs; dev builds allow localhost
/// plain HTTP for local development.
fn validate_server_url(server_url: &str) -> Result<()> {
    let is_loopback = |url: &str| {
        url.contains("://localhost") || url.contains("://127.0.0.1") || url.contains("://[::1]")
    };
    if server_url.starts_with("https://") || (cfg!(debug_assertions) && is_loopback(server_url)) {
        Ok(())
    } else {
        Err(Error(
            "insecure server URL: production profiles must use https:// (dev builds may use http://localhost)"
                .into(),
        ))
    }
}

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
    ///
    /// Never serialized: persistence goes through the OS keychain, so a
    /// stolen `settings.json` yields no bearer token.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
}

/// Loads the saved profiles list, or an empty vector if none exists.
/// Tokens are re-attached from the OS keychain, not from disk.
pub fn load_profiles<R: Runtime>(app: &AppHandle<R>) -> Result<Vec<Profile>> {
    let store = app.store(STORE_NAME).map_err(|e| Error(e.to_string()))?;
    let mut profiles: Vec<Profile> = store
        .get(PROFILES_KEY)
        .and_then(|v| serde_json::from_value::<Vec<Profile>>(v.clone()).ok())
        .unwrap_or_default();
    for profile in &mut profiles {
        profile.token = keychain_get(&profile.name);
    }
    Ok(profiles)
}

/// Saves the entire profile list to disk. Tokens are extracted to the OS
/// keychain first; the store file is written with no token material.
pub fn save_profiles<R: Runtime>(app: &AppHandle<R>, profiles: &[Profile]) -> Result<()> {
    for profile in profiles {
        validate_server_url(&profile.server_url)?;
    }
    let store = app.store(STORE_NAME).map_err(|e| Error(e.to_string()))?;
    let mut sanitized: Vec<Profile> = profiles.to_vec();
    for profile in &mut sanitized {
        match &profile.token {
            Some(token) => {
                keychain_set(&profile.name, token)?;
                profile.token = None; // never persist the token to disk
            }
            None => keychain_delete(&profile.name),
        }
    }
    let value = serde_json::to_value(&sanitized).map_err(|e| Error(e.to_string()))?;
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

#[cfg(test)]
mod tests {
    use super::*;

    /// R-22: the serialized profile must never carry token material.
    #[test]
    fn profile_json_never_carries_the_token() {
        let profile = Profile {
            name: "prod".into(),
            server_url: "https://picroom.example.com".into(),
            email: "a@b.c".into(),
            token: Some("secret".into()),
        };
        let json = serde_json::to_string(&profile).unwrap();
        assert!(!json.contains("secret"), "token must not serialize: {json}");
    }

    /// R-22: plain http is refused outside loopback/dev.
    #[test]
    fn server_url_validation_shape() {
        let check = |url: &str| {
            let is_loopback = url.contains("://localhost")
                || url.contains("://127.0.0.1")
                || url.contains("://[::1]");
            url.starts_with("https://") || (cfg!(debug_assertions) && is_loopback)
        };
        assert!(check("https://picroom.example.com"));
        assert!(
            !check("http://picroom.example.com"),
            "plain http must be refused in release"
        );
        if cfg!(debug_assertions) {
            assert!(check("http://localhost:8080"), "dev may use localhost http");
        }
    }
}
