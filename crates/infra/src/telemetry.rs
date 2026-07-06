//! Prometheus metrics.

use metrics::{describe_counter, describe_gauge, describe_histogram, Unit};

/// Initialises metric descriptions (does not bind an exporter; that happens
/// in the API crate).
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
}
