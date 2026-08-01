# Picroom Admin Client

A desktop administration client for [Picroom](https://github.com/picroom/picroom), built with
[Tauri 2](https://v2.tauri.app/) and [Vue 3](https://vuejs.org/). It talks to the Picroom REST API
to manage images, users, teams, storage policies, and audit logs from a native window.

## Architecture

- **Frontend**: Vue 3 + TypeScript + Vite, [Naive UI](https://www.naiveui.com/) for components,
  Pinia for state, and `vue-router` for navigation. JSON API calls are issued through
  `tauri-plugin-http` (native fetch, no CORS).
- **Backend bridge**: Rust Tauri commands in `src-tauri/src/commands/` stream large file
  upload/download and persist server profiles to a `tauri-plugin-store` settings file.
- **`src-tauri` is a standalone Cargo workspace** (not part of the main Picroom workspace), so the
  desktop client builds independently of the server crates.

## Prerequisites

- [Rust](https://www.rust-lang.org/) (stable) with the WebView2 toolchain for Tauri 2.
- [Node.js](https://nodejs.org/) 18+ and npm.
- On Windows, the [Microsoft C++ Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/)
  and WebView2 runtime.

## Development

```bash
npm install
npm run dev          # start the Vite dev server + Tauri dev window
```

## Tests & lint

```bash
npm run lint         # vue-tsc type-check + ESLint
npm test             # vitest unit tests (frontend)
cd src-tauri && cargo test        # Rust command-layer unit tests (wiremock)
cd src-tauri && cargo clippy --all-targets   # Rust lint
```

## Building a release bundle

```bash
npm run tauri build  # produces a platform installer under src-tauri/target/release/
```

## Project layout

```
desktop/
├── index.html
├── package.json
├── vite.config.ts
├── src/
│   ├── main.ts
│   ├── App.vue
│   ├── router/            # routes + auth guard
│   ├── stores/            # Pinia stores (auth)
│   ├── api/               # HTTP client + typed endpoints
│   ├── views/             # Login / Images / Users / Teams / Audit / Storage / Settings
│   └── components/        # CopyLinkButton / ImageGrid / UploadDropzone
└── src-tauri/
    ├── Cargo.toml
    ├── tauri.conf.json
    └── src/
        ├── lib.rs
        ├── error.rs       # client error type
        ├── state.rs       # login types + request
        ├── store.rs       # profile persistence
        └── commands/      # auth / upload / download
```
