// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Picroom Contributors

//! Prometheus metrics.
//!
//! A `metrics-exporter-prometheus` recorder is installed once at startup; the
//! handle is kept in a static so the `/metrics` handler can render the current
//! exposition format on demand without threading state through handlers.

use metrics::{describe_counter, describe_gauge, describe_histogram, Unit};
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use std::sync::OnceLock;

static HANDLE: OnceLock<PrometheusHandle> = OnceLock::new();

/// Installs the Prometheus recorder and registers metric descriptions.
/// Safe to call once at process start; subsequent calls are no-ops.
pub fn init_metrics() {
    describe_counter!("picroom_http_requests_total", "Total HTTP requests");
    describe_counter!("picroom_uploads_total", "Total image uploads");
    describe_counter!(
        "picroom_storage_bytes",
        Unit::Bytes,
        "Total bytes in storage"
    );
    describe_histogram!("picroom_upload_duration_seconds", "Upload duration");
    describe_gauge!("picroom_in_flight_uploads", "Currently in-flight uploads");
    describe_gauge!("picroom_worker_queue_depth", "Pending worker jobs");

    if HANDLE.get().is_none() {
        match PrometheusBuilder::new().install_recorder() {
            Ok(handle) => {
                let _ = HANDLE.set(handle);
            }
            Err(e) => {
                tracing::warn!(error = %e, "failed to install Prometheus recorder");
            }
        }
    }
}

/// Renders the current metrics in Prometheus exposition format.
/// Returns a notice string if the recorder was never installed.
pub fn render_metrics() -> String {
    HANDLE.get().map_or_else(
        || "# Prometheus recorder not installed\n".to_string(),
        PrometheusHandle::render,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_metrics_exposes_recorded_values() {
        init_metrics();
        // A fresh recorder exposes nothing until a value is recorded.
        metrics::counter!("picroom_test_counter_total").increment(1);
        let out = render_metrics();
        assert!(
            out.contains("picroom_test_counter_total"),
            "expected rendered metrics to include the counter, got:\n{out}"
        );
    }
}
