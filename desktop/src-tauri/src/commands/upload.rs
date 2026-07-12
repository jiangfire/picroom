// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Picroom Contributors

//! Streaming upload command for the Picroom admin client.

use futures_util::StreamExt;
use serde::Serialize;
use tauri::ipc::Channel;
use tauri::AppHandle;
use tokio::fs::File;
use tokio_util::io::ReaderStream;

use crate::config;

/// Progress event emitted during an upload.
#[derive(Clone, Serialize)]
pub struct UploadProgress {
    /// Event kind: `"progress"`, `"done"`, or `"error"`.
    pub kind: String,
    /// Bytes sent so far, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bytes_sent: Option<u64>,
    /// Total bytes to send, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_bytes: Option<u64>,
    /// Error message when kind is `"error"`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Uploads a file to the active Picroom server via a streaming multipart POST.
///
/// Progress events are sent on the provided channel. The returned JSON is the
/// server image metadata (id, content_type, dimensions, etc.).
#[tauri::command]
pub async fn upload_file(
    app: AppHandle,
    file_path: String,
    team_id: Option<String>,
    on_progress: Channel<UploadProgress>,
) -> Result<serde_json::Value, String> {
    let profile = config::load_active_profile(&app)?;
    let profile = profile.ok_or("no active profile")?;
    let token = profile.token.as_ref().ok_or("not logged in")?;
    let server_url = profile.server_url.trim_end_matches('/').to_string();
    let upload_url = format!("{server_url}/api/v1/images");

    let file = File::open(&file_path).await.map_err(|e| e.to_string())?;
    let total = file.metadata().await.map_err(|e| e.to_string())?.len();
    let file_name = std::path::Path::new(&file_path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("file")
        .to_string();
    let mime = mime_guess::from_path(&file_path)
        .first_or_octet_stream()
        .to_string();

    let progress = on_progress.clone();
    let mut bytes_sent: u64 = 0;
    let reader = ReaderStream::new(file).map(move |chunk| {
        let bytes = chunk?;
        bytes_sent += bytes.len() as u64;
        let _ = progress.send(UploadProgress {
            kind: "progress".to_string(),
            bytes_sent: Some(bytes_sent),
            total_bytes: Some(total),
            error: None,
        });
        Ok::<_, std::io::Error>(bytes)
    });

    let part = reqwest::multipart::Part::stream(reqwest::Body::wrap_stream(reader))
        .file_name(file_name)
        .mime_str(&mime)
        .map_err(|e| e.to_string())?;
    let mut form = reqwest::multipart::Form::new().part("file", part);
    if let Some(tid) = team_id {
        form = form.text("team_id", tid);
    }

    let client = reqwest::Client::new();
    let response = client
        .post(&upload_url)
        .header("Authorization", format!("Bearer {token}"))
        .multipart(form)
        .send()
        .await
        .map_err(|e| {
            let _ = on_progress.send(UploadProgress {
                kind: "error".to_string(),
                bytes_sent: Some(bytes_sent),
                total_bytes: Some(total),
                error: Some(e.to_string()),
            });
            e.to_string()
        })?;

    if !response.status().is_success() {
        let msg = format!("upload failed: {}", response.status());
        let _ = on_progress.send(UploadProgress {
            kind: "error".to_string(),
            bytes_sent: Some(total),
            total_bytes: Some(total),
            error: Some(msg.clone()),
        });
        return Err(msg);
    }

    let json: serde_json::Value = response.json().await.map_err(|e| e.to_string())?;
    let _ = on_progress.send(UploadProgress {
        kind: "done".to_string(),
        bytes_sent: Some(total),
        total_bytes: Some(total),
        error: None,
    });
    Ok(json)
}
