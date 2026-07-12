// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Picroom Contributors

//! Streaming download command for the Picroom admin client.

use tauri::AppHandle;
use tokio::io::AsyncWriteExt;

use crate::config;

/// Downloads the image with the given id to `save_path`.
///
/// The link endpoint is queried first to resolve the public URL, then the bytes
/// are streamed to disk without loading the whole file into memory.
#[tauri::command]
pub async fn download_image(
    app: AppHandle,
    image_id: String,
    save_path: String,
) -> Result<(), String> {
    let profile = config::load_active_profile(&app)?;
    let profile = profile.ok_or("no active profile")?;
    let token = profile.token.as_ref().ok_or("not logged in")?;
    let server_url = profile.server_url.trim_end_matches('/').to_string();

    let link_url = format!("{server_url}/api/v1/images/{image_id}/link");
    let client = reqwest::Client::new();
    let link_response = client
        .get(&link_url)
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !link_response.status().is_success() {
        return Err(format!("link failed: {}", link_response.status()));
    }
    let link_json: serde_json::Value = link_response.json().await.map_err(|e| e.to_string())?;
    let public_url = link_json["public_url"]
        .as_str()
        .ok_or("missing public_url")?;

    let url = if public_url.starts_with("http") {
        public_url.to_string()
    } else {
        format!("{server_url}{public_url}")
    };

    let mut response = client.get(&url).send().await.map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        return Err(format!("download failed: {}", response.status()));
    }

    let mut file = tokio::fs::File::create(&save_path)
        .await
        .map_err(|e| e.to_string())?;
    while let Some(chunk) = response.chunk().await.map_err(|e| e.to_string())? {
        file.write_all(&chunk).await.map_err(|e| e.to_string())?;
    }

    Ok(())
}
