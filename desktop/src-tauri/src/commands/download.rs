// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Picroom Contributors

//! Streaming download command for the Picroom admin client.

use tauri::AppHandle;
use tokio::io::AsyncWriteExt;

use crate::config;

/// Resolves the public URL for an image and streams the bytes to `save_path`.
async fn perform_download(
    server_url: &str,
    token: &str,
    image_id: &str,
    save_path: &str,
    client: &reqwest::Client,
) -> Result<(), String> {
    let server_url = server_url.trim_end_matches('/');
    let link_url = format!("{server_url}/api/v1/images/{image_id}/link");

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

    let mut file = tokio::fs::File::create(save_path)
        .await
        .map_err(|e| e.to_string())?;
    while let Some(chunk) = response.chunk().await.map_err(|e| e.to_string())? {
        file.write_all(&chunk).await.map_err(|e| e.to_string())?;
    }

    Ok(())
}

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

    perform_download(
        &profile.server_url,
        token,
        &image_id,
        &save_path,
        &reqwest::Client::new(),
    )
    .await
}

#[cfg(test)]
mod tests {
    use std::io::Read;

    use super::*;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    async fn assert_downloaded(
        server: &MockServer,
        image_id: &str,
        image_key: &str,
        public_url: String,
        body: &[u8],
        file_name: &str,
    ) {
        Mock::given(method("GET"))
            .and(path(format!("/api/v1/images/{image_id}/link")))
            .and(header("Authorization", "Bearer token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "public_url": public_url,
            })))
            .mount(server)
            .await;

        Mock::given(method("GET"))
            .and(path(format!("/i/{image_key}")))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(body))
            .mount(server)
            .await;

        let temp_dir = std::env::temp_dir();
        let save_path = temp_dir.join(file_name);
        let _ = tokio::fs::remove_file(&save_path).await;

        let client = reqwest::Client::new();
        perform_download(
            &server.uri(),
            "token",
            image_id,
            save_path.to_str().unwrap(),
            &client,
        )
        .await
        .unwrap();

        let mut contents = Vec::new();
        std::fs::File::open(&save_path)
            .unwrap()
            .read_to_end(&mut contents)
            .unwrap();
        assert_eq!(contents, body);

        tokio::fs::remove_file(&save_path).await.unwrap();
    }

    #[tokio::test]
    async fn perform_download_streams_absolute_and_relative_urls() {
        let server = MockServer::start().await;

        assert_downloaded(
            &server,
            "img-123",
            "img/key.bin",
            format!("{}/i/img/key.bin", server.uri()),
            b"hello image",
            "picroom-download-test.bin",
        )
        .await;

        assert_downloaded(
            &server,
            "img-456",
            "img/relative.bin",
            "/i/img/relative.bin".to_string(),
            b"relative",
            "picroom-download-relative-test.bin",
        )
        .await;
    }
}
