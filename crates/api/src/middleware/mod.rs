//! Middleware (trace, auth).

pub mod auth;
pub mod trace;

pub use auth::require_auth;
pub use trace::{request_id_layer, trace_layer};