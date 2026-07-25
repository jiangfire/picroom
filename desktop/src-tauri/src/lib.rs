// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Picroom Contributors

//! Picroom admin client — Tauri command layer.
//!
//! The client is a thin HTTP client over the Picroom REST API. JSON CRUD is
//! issued from the frontend via `tauri-plugin-http` (native fetch, no CORS);
//! large file upload/download streams through Rust commands
//! (`commands/upload.rs`, `commands/download.rs`). See `docs/spec-admin-client.md`.

mod commands;
mod error;
mod state;
mod store;

use commands::{auth, download, upload};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_http::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .invoke_handler(tauri::generate_handler![
            auth::login,
            auth::logout,
            auth::get_session,
            auth::list_profiles,
            auth::save_profile,
            auth::set_active_profile,
            auth::remove_profile,
            upload::upload_file,
            download::download_image,
        ])
        .run(tauri::generate_context!())
        .expect("error while running picroom admin client");
}
