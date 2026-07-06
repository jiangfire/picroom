# Changelog

All notable changes to Picroom are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Initial spec, plan, task breakdown, ADRs.
- Workspace skeleton with 11 crates + 1 binary crate.
- CI/CD pipeline (fmt + clippy + test + audit + deny + coverage).
- Docker Compose stack for local development.
- Multi-stage Dockerfile producing distroless runtime image.
- Example configuration (`docker/config.example.toml`).
- OpenAPI 3.1 specification (`docs/api/openapi.yaml`).

### Security
- All Rust dependencies pinned to MIT-compatible licenses via `cargo deny`.