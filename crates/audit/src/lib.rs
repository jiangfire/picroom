//! # Picroom Audit
//!
//! Append-only audit log: events + sinks.

#![allow(missing_docs)]

pub mod db_sink;
pub mod event;
pub mod sink;

pub use db_sink::DbAuditSink;
pub use event::{AuditAction, AuditEvent};
pub use sink::{AuditSink, InMemoryAuditSink, NoopAuditSink};
