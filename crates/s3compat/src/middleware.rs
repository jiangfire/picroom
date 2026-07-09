// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Picroom Contributors

//! `SigV4` verification middleware.
//!
//! When the application state provides an [`S3Credential`](crate::S3Credential),
//! every request to the S3-compatible surface is signature-checked against it.
//! When no credential is configured the middleware is a pass-through, preserving
//! the existing dev-mode behaviour. This wires the previously-dead
//! [`sigv4::verify`](crate::sigv4::verify) into the live request path.

use crate::sigv4::{parse_authz, verify};
use crate::{S3Credential, S3Error, S3State};
use axum::body::Body;
use axum::extract::State;
use axum::http::Request;
use axum::middleware::Next;
use axum::response::Response;
use std::sync::Arc;

/// Middleware: enforce `SigV4` on `/s3/*` when credentials are configured.
pub async fn require_sigv4<S>(
    State(state): State<Arc<S>>,
    req: Request<Body>,
    next: Next,
) -> Result<Response, S3Error>
where
    S: S3State,
{
    let Some(creds) = state.s3_credentials() else {
        // Dev mode: no credentials configured → open S3 endpoint.
        return Ok(next.run(req).await);
    };
    verify_request(&req, &creds)?;
    Ok(next.run(req).await)
}

/// Verifies the `SigV4` signature on `req` against `creds`. Also used by tests
/// (with a request signed via the crate's own [`sign`] primitive).
pub(crate) fn verify_request(req: &Request<Body>, creds: &S3Credential) -> Result<(), S3Error> {
    let headers = req.headers();

    let authz = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| S3Error::BadRequest("missing Authorization".into()))?;
    let parsed = parse_authz(authz)?;

    // The access key must match the configured credential.
    if parsed.access_key != creds.access_key {
        return Err(S3Error::SignatureMismatch);
    }

    let amz_date = headers
        .get("x-amz-date")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| S3Error::BadRequest("missing x-amz-date".into()))?;

    // Payload hash: clients either send x-amz-content-sha256 or mark the
    // payload unsigned. We trust the header value (standard for SigV4).
    let payload_hash = headers
        .get("x-amz-content-sha256")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("UNSIGNED-PAYLOAD");

    let canonical_uri = req.uri().path().to_string();
    let canonical_query = canonical_query_string(req.uri().query());
    let canonical_headers = canonical_headers(&parsed.signed_headers, headers);

    verify(
        &parsed,
        req.method().as_str(),
        &canonical_uri,
        &canonical_query,
        &canonical_headers,
        payload_hash,
        &creds.secret,
        amz_date,
    )
}

/// Builds the canonical query string: `k=v` pairs sorted by key, joined by `&`.
fn canonical_query_string(query: Option<&str>) -> String {
    let Some(q) = query else {
        return String::new();
    };
    let mut pairs: Vec<(&str, &str)> = q.split('&').filter_map(|p| p.split_once('=')).collect();
    pairs.sort_unstable();
    pairs
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join("&")
}

/// Builds the canonical headers block: `name:value\n` for each signed header.
fn canonical_headers(signed: &[String], headers: &axum::http::HeaderMap) -> String {
    let mut out = String::new();
    for name in signed {
        let lower = name.to_lowercase();
        if let Some(v) = headers.get(&lower).and_then(|v| v.to_str().ok()) {
            out.push_str(&lower);
            out.push(':');
            out.push_str(v.trim());
            out.push('\n');
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sigv4::{canonical_request, derive_signing_key, sign, string_to_sign};
    use axum::http::Request;

    /// Sign a request with our own primitives, then confirm `verify_request`
    /// accepts it — and rejects a tampered signature.
    #[test]
    fn verify_request_accepts_well_signed_and_rejects_tampered() {
        let creds = S3Credential {
            access_key: "AKIDTEST".into(),
            secret: "s3cr3t".into(),
        };
        let date = "20240101";
        let amz_date = "20240101T000000Z";
        let region = "us-east-1";
        let service = "s3";

        let signed_headers = [
            "host".to_string(),
            "x-amz-content-sha256".to_string(),
            "x-amz-date".to_string(),
        ];

        // Canonical inputs the signer and verifier must agree on.
        let canonical_uri = "/picroom/test.bin";
        let canonical_query = "";
        let canonical_headers = format!(
            "host:localhost:8080\nx-amz-content-sha256:UNSIGNED-PAYLOAD\nx-amz-date:{amz_date}\n"
        );
        let payload_hash = "UNSIGNED-PAYLOAD";

        let date_scope = format!("{date}/{region}/{service}/aws4_request");
        let canonical = canonical_request(
            "GET",
            canonical_uri,
            canonical_query,
            &canonical_headers,
            &signed_headers.join(";"),
            payload_hash,
        );
        let s2s = string_to_sign("AWS4-HMAC-SHA256", amz_date, &date_scope, &canonical);
        let key = derive_signing_key(&creds.secret, date, region, service);
        let signature = sign(&key, &s2s);

        let authz =
            format!(
            "AWS4-HMAC-SHA256 Credential={}/{}/{}/{}/aws4_request, SignedHeaders={}, Signature={}",
            creds.access_key, date, region, service,
            signed_headers.join(";"),
            signature
        );

        let req = Request::builder()
            .method("GET")
            .uri(canonical_uri)
            .header("host", "localhost:8080")
            .header("x-amz-date", amz_date)
            .header("x-amz-content-sha256", "UNSIGNED-PAYLOAD")
            .header("authorization", authz)
            .body(Body::empty())
            .unwrap();

        assert!(
            verify_request(&req, &creds).is_ok(),
            "well-signed request must verify"
        );

        // Tamper: flip one hex char in the signature.
        let tampered_sig = if signature.starts_with('a') {
            signature.replacen('a', "b", 1)
        } else {
            signature.replacen('0', "1", 1)
        };
        let bad_authz =
            format!(
            "AWS4-HMAC-SHA256 Credential={}/{}/{}/{}/aws4_request, SignedHeaders={}, Signature={}",
            creds.access_key, date, region, service,
            signed_headers.join(";"),
            tampered_sig
        );
        let req_bad = Request::builder()
            .method("GET")
            .uri(canonical_uri)
            .header("host", "localhost:8080")
            .header("x-amz-date", amz_date)
            .header("x-amz-content-sha256", "UNSIGNED-PAYLOAD")
            .header("authorization", bad_authz)
            .body(Body::empty())
            .unwrap();
        assert!(
            matches!(
                verify_request(&req_bad, &creds),
                Err(S3Error::SignatureMismatch)
            ),
            "tampered signature must be rejected"
        );
    }

    #[test]
    fn verify_request_rejects_wrong_access_key() {
        let creds = S3Credential {
            access_key: "AKIDTEST".into(),
            secret: "s3cr3t".into(),
        };
        let authz = "AWS4-HMAC-SHA256 Credential=OTHERKEY/20240101/us-east-1/s3/aws4_request, SignedHeaders=host, Signature=abc";
        let req = Request::builder()
            .method("GET")
            .uri("/")
            .header("host", "localhost")
            .header("x-amz-date", "20240101T000000Z")
            .header("authorization", authz)
            .body(Body::empty())
            .unwrap();
        assert!(matches!(
            verify_request(&req, &creds),
            Err(S3Error::SignatureMismatch)
        ));
    }
}
