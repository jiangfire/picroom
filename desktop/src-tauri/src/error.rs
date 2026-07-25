// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Picroom Contributors

//! Error type for the Picroom admin client command layer.

use serde::Serialize;
use tauri::Error as TauriError;

/// Client-facing error.
///
/// Always serializes to a plain string so Tauri commands surface a readable
/// message to the frontend instead of a serialized error struct.
#[derive(Debug, Clone, Serialize)]
pub struct Error(pub String);

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

/// Convenience result alias used by all command-layer functions.
pub type Result<T> = std::result::Result<T, Error>;

impl From<String> for Error {
    fn from(value: String) -> Self {
        Error(value)
    }
}

impl From<&str> for Error {
    fn from(value: &str) -> Self {
        Error(value.to_string())
    }
}

impl From<TauriError> for Error {
    fn from(value: TauriError) -> Self {
        Error(value.to_string())
    }
}

impl From<serde_json::Error> for Error {
    fn from(value: serde_json::Error) -> Self {
        Error(value.to_string())
    }
}
