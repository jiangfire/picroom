// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Picroom Contributors

//! Login request/result types and the login HTTP request.

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

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
pub async fn perform_login(
    server_url: &str,
    email: &str,
    password: &str,
    client: &reqwest::Client,
) -> Result<LoginResult> {
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
        return Err(Error(format!("login failed: {}", response.status())));
    }

    let json: serde_json::Value = response.json().await.map_err(|e| Error(e.to_string()))?;
    let token = json["access_token"]
        .as_str()
        .ok_or_else(|| Error("missing access_token in response".to_string()))?
        .to_string();

    Ok(LoginResult {
        email: email.to_string(),
        server_url,
        token,
    })
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
        assert!(result.unwrap_err().0.contains("401"));
    }
}
