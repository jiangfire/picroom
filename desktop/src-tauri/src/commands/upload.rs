// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Picroom Contributors

//! Streaming upload command for the Picroom admin client.

#[cfg(test)]
use std::sync::{Arc, Mutex};

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

/// Sink for upload progress events.
pub trait ProgressSink: Clone + Send {
    fn send_progress(&self, progress: UploadProgress);
}

impl ProgressSink for Channel<UploadProgress> {
    fn send_progress(&self, progress: UploadProgress) {
        let _ = self.send(progress);
    }
}

#[cfg(test)]
#[derive(Clone, Default)]
struct TestSink(Arc<Mutex<Vec<UploadProgress>>>);

#[cfg(test)]
impl ProgressSink for TestSink {
    fn send_progress(&self, progress: UploadProgress) {
        self.0.lock().unwrap().push(progress);
    }
}

async fn perform_upload<P: ProgressSink + 'static>(
    server_url: &str,
    token: &str,
    file_path: &str,
    team_id: Option<String>,
    on_progress: P,
    client: &reqwest::Client,
) -> Result<serde_json::Value, String> {
    let server_url = server_url.trim_end_matches('/').to_string();
    let upload_url = format!("{server_url}/api/v1/images");

    let file = File::open(file_path).await.map_err(|e| e.to_string())?;
    let total = file.metadata().await.map_err(|e| e.to_string())?.len();
    let file_name = std::path::Path::new(file_path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("file")
        .to_string();
    let mime = mime_guess::from_path(file_path)
        .first_or_octet_stream()
        .to_string();

    let progress = on_progress.clone();
    let mut bytes_sent: u64 = 0;
    let reader = ReaderStream::new(file).map(move |chunk| {
        let bytes = chunk?;
        bytes_sent += bytes.len() as u64;
        progress.send_progress(UploadProgress {
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

    let response = client
        .post(&upload_url)
        .header("Authorization", format!("Bearer {token}"))
        .multipart(form)
        .send()
        .await
        .map_err(|e| {
            on_progress.send_progress(UploadProgress {
                kind: "error".to_string(),
                bytes_sent: Some(bytes_sent),
                total_bytes: Some(total),
                error: Some(e.to_string()),
            });
            e.to_string()
        })?;

    if !response.status().is_success() {
        let msg = format!("upload failed: {}", response.status());
        on_progress.send_progress(UploadProgress {
            kind: "error".to_string(),
            bytes_sent: Some(total),
            total_bytes: Some(total),
            error: Some(msg.clone()),
        });
        return Err(msg);
    }

    let json: serde_json::Value = response.json().await.map_err(|e| e.to_string())?;
    on_progress.send_progress(UploadProgress {
        kind: "done".to_string(),
        bytes_sent: Some(total),
        total_bytes: Some(total),
        error: None,
    });
    Ok(json)
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

    perform_upload(
        &profile.server_url,
        token,
        &file_path,
        team_id,
        on_progress,
        &reqwest::Client::new(),
    )
    .await
}

#[cfg(test)]
mod tests {
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;

    #[tokio::test]
    async fn perform_upload_streams_file_to_server() {
        let server = MockServer::start().await;
        let response = serde_json::json!({
            "id": "img-789",
            "bytes": 11,
            "width": 100,
            "height": 100,
            "content_type": "image/png",
        });
        Mock::given(method("POST"))
            .and(path("/api/v1/images"))
            .and(header("Authorization", "Bearer token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(response))
            .mount(&server)
            .await;

        let temp_dir = std::env::temp_dir();
        let file_path = temp_dir.join("picroom-upload-test.png");
        tokio::fs::write(&file_path, b"hello image").await.unwrap();

        let sink = TestSink::default();
        let client = reqwest::Client::new();
        let result = perform_upload(
            &server.uri(),
            "token",
            file_path.to_str().unwrap(),
            Some("team-1".to_string()),
            sink.clone(),
            &client,
        )
        .await
        .unwrap();

        assert_eq!(result["id"], "img-789");
        assert_eq!(result["content_type"], "image/png");

        let events = sink.0.lock().unwrap();
        let progress_events: Vec<_> = events.iter().filter(|e| e.kind == "progress").collect();
        assert!(!progress_events.is_empty());
        assert_eq!(events.last().unwrap().kind, "done");

        tokio::fs::remove_file(&file_path).await.unwrap();
    }
}
