// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Picroom Contributors

//! Router construction.

use crate::state::AppState;
use axum::middleware;
use axum::routing::{get, patch, post};
use axum::Router;
use std::sync::Arc;

/// Builds the root router with all v1 endpoints, the auth middleware,
/// and the S3-compat surface.
pub fn build_router(state: Arc<AppState>) -> Router {
    Router::new()
        // System (open to all)
        .route("/healthz", get(super::handlers::system::healthz))
        .route("/readyz", get(super::handlers::system::readyz))
        .route("/metrics", get(super::handlers::system::metrics))
        // Auth (open)
        .route("/api/v1/auth/login", post(super::handlers::auth::login))
        .route("/api/v1/auth/logout", post(super::handlers::auth::logout))
        // API v1 (requires auth middleware)
        .route(
            "/api/v1/images",
            get(super::handlers::images::list).post(super::handlers::images::upload),
        )
        .route(
            "/api/v1/images/:id",
            get(super::handlers::images::get).delete(super::handlers::images::delete),
        )
        .route("/api/v1/teams", post(super::handlers::teams::create))
        .route("/api/v1/teams/:id", get(super::handlers::teams::get))
        .route(
            "/api/v1/teams/:id/members",
            post(super::handlers::teams::add_member),
        )
        .route(
            "/api/v1/admin/users",
            post(super::handlers::admin::create_user),
        )
        .route(
            "/api/v1/admin/users/:id/role",
            patch(super::handlers::admin::set_role),
        )
        .route("/api/v1/audit", get(super::handlers::admin::audit))
        // S3-compat (open — SigV4 verified per-request)
        .nest(
            "/s3",
            picroom_s3compat::s3_router::<AppState>(state.clone()),
        )
        // Auth middleware on all /api/v1/* routes (login/logout excluded).
        // Verifies the JWT signature so forged tokens are rejected at the gate.
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            crate::extractors::auth::require_auth::<Arc<AppState>>,
        ))
        // Shared state
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn build_router_returns_router() {
        let _r = build_router(Arc::new(AppState::for_dev(
            Arc::new(picroom_storage::driver::LocalDriver::new(
                std::path::PathBuf::from("/tmp"),
                "/i",
            )),
            Arc::new(picroom_audit::NoopAuditSink),
        )));
    }
}
