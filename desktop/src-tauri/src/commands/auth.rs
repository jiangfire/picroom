// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Picroom Contributors

//! Tauri commands for Picroom admin authentication and session management.

use serde::{Deserialize, Serialize};
use tauri::AppHandle;

use crate::config;
use crate::config::Profile;

#[derive(Debug, Deserialize)]
pub struct LoginPayload {
    pub server_url: String,
    pub email: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct LoginResult {
    pub email: String,
    pub server_url: String,
    pub token: String,
}

#[derive(Debug, Serialize)]
pub struct Session {
    pub name: String,
    pub server_url: String,
    pub email: String,
    pub token: String,
}

/// Executes the login HTTP request against a Picroom server.
async fn perform_login(
    server_url: &str,
    email: &str,
    password: &str,
    client: &reqwest::Client,
) -> Result<LoginResult, String> {
    let server_url = server_url.trim_end_matches('/').to_string();
    let login_url = format!("{server_url}/api/v1/auth/login");

    let body = serde_json::json!({
        "email": email,
        "password": password,
    });

    let response = client
        .post(&login_url)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("request failed: {e}"))?;

    if !response.status().is_success() {
        return Err(format!("login failed: {}", response.status()));
    }

    let json: serde_json::Value = response.json().await.map_err(|e| e.to_string())?;
    let token = json["access_token"]
        .as_str()
        .ok_or("missing access_token in response")?
        .to_string();

    Ok(LoginResult {
        email: email.to_string(),
        server_url,
        token,
    })
}

#[tauri::command]
pub async fn login(app: AppHandle, payload: LoginPayload) -> Result<LoginResult, String> {
    let client = reqwest::Client::new();
    let result = perform_login(
        &payload.server_url,
        &payload.email,
        &payload.password,
        &client,
    )
    .await?;

    upsert_profile(&app, &result.server_url, &result.email, &result.token)?;
    config::set_active_profile_name(&app, Some("default"))?;

    Ok(result)
}

fn upsert_profile(
    app: &AppHandle,
    server_url: &str,
    email: &str,
    token: &str,
) -> Result<(), String> {
    let mut profiles = config::load_profiles(app)?;
    let mut found = false;
    for profile in &mut profiles {
        if profile.name == "default" {
            profile.server_url = server_url.to_string();
            profile.email = email.to_string();
            profile.token = Some(token.to_string());
            found = true;
            break;
        }
    }
    if !found {
        profiles.push(Profile {
            name: "default".into(),
            server_url: server_url.into(),
            email: email.into(),
            token: Some(token.into()),
        });
    }
    config::save_profiles(app, &profiles)
}

#[tauri::command]
pub fn logout(app: AppHandle) -> Result<(), String> {
    let mut profiles = config::load_profiles(&app)?;
    let active = config::active_profile_name(&app)?;
    if let Some(ref name) = active {
        if let Some(profile) = profiles.iter_mut().find(|p| &p.name == name) {
            profile.token = None;
        }
    }
    config::save_profiles(&app, &profiles)?;
    config::set_active_profile_name(&app, None)
}

#[tauri::command]
pub fn get_session(app: AppHandle) -> Result<Option<Session>, String> {
    let profile = config::load_active_profile(&app)?;
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
pub fn list_profiles(app: AppHandle) -> Result<Vec<Profile>, String> {
    config::load_profiles(&app)
}

#[tauri::command]
pub fn save_profile(app: AppHandle, profile: Profile) -> Result<(), String> {
    let mut profiles = config::load_profiles(&app)?;
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
    config::save_profiles(&app, &profiles)
}

#[tauri::command]
pub fn set_active_profile(app: AppHandle, name: String) -> Result<(), String> {
    config::set_active_profile_name(&app, Some(&name))
}

#[tauri::command]
pub fn remove_profile(app: AppHandle, name: String) -> Result<(), String> {
    let mut profiles = config::load_profiles(&app)?;
    profiles.retain(|p| p.name != name);
    config::save_profiles(&app, &profiles)?;
    if config::active_profile_name(&app)?.as_ref() == Some(&name) {
        config::set_active_profile_name(&app, None)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{body_json, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn perform_login_returns_token() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v1/auth/login"))
            .and(body_json(serde_json::json!({
                "email": "admin@example.com",
                "password": "secret",
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": "abc123",
            })))
            .mount(&server)
            .await;

        let client = reqwest::Client::new();
        let result = perform_login(&server.uri(), "admin@example.com", "secret", &client)
            .await
            .unwrap();

        assert_eq!(result.email, "admin@example.com");
        assert_eq!(result.token, "abc123");
        assert!(result.server_url.starts_with("http://127.0.0.1"));
    }

    #[tokio::test]
    async fn perform_login_propagates_error_status() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v1/auth/login"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&server)
            .await;

        let client = reqwest::Client::new();
        let result = perform_login(&server.uri(), "admin@example.com", "secret", &client).await;

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("401"));
    }
}
