// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Picroom Contributors

//! `SigV4` verification middleware.
//!
//! When the application state provides an [`S3Credential`](crate::S3Credential),
//! every request to the S3-compatible surface is signature-checked against it.
//! When no credential is configured the middleware is a pass-through, preserving
//! the existing dev-mode behaviour. This wires the previously-dead
//! [`sigv4::verify`](crate::sigv4::verify) into the live request path.
//!
//! Hardening (R-03 / R-07):
//! - signatures older than 15 minutes are rejected (`within_skew`), so a
//!   captured `Authorization` header cannot be replayed forever;
//! - when the client declares a payload hash (`x-amz-content-sha256`), the
//!   received body is hashed and compared — a signed request cannot have its
//!   body swapped in transit;
//! - `SignedHeaders` must include `host` and `x-amz-date`, and every declared
//!   header must actually be present, else the signature is rejected;
//! - the canonical URI is percent-normalised the way AWS specifies.

use crate::sigv4::{parse_authz, sha256_hex, verify, within_skew};
use crate::{S3Credential, S3Error, S3State};
use axum::body::Body;
use axum::extract::State;
use axum::http::request::Parts;
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
    // Buffer the body so the declared payload hash can be checked against the
    // bytes actually received. Object handlers buffer anyway (and the body
    // limit bounds the size), so this does not change the memory profile.
    let (parts, body) = req.into_parts();
    let bytes = axum::body::to_bytes(body, usize::MAX)
        .await
        .map_err(|_| S3Error::BadRequest("failed to read request body".into()))?;
    verify_request(&parts, &bytes, &creds)?;
    Ok(next
        .run(Request::from_parts(parts, Body::from(bytes)))
        .await)
}

/// Verifies the `SigV4` signature on `req` against `creds`, including the
/// body-hash check. Used by the middleware and tests (with a request signed
/// via the crate's own [`sign`](crate::sigv4::sign) primitive).
pub(crate) fn verify_request(
    req: &Parts,
    body: &[u8],
    creds: &S3Credential,
) -> Result<(), S3Error> {
    let headers = &req.headers;

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

    // R-03: replay protection — a signature older (or newer) than the skew
    // window is rejected outright.
    if !within_skew(amz_date, time::OffsetDateTime::now_utc()) {
        return Err(S3Error::SignatureMismatch);
    }

    // R-07: `host` and `x-amz-date` must be signed, or the signature binds to
    // neither the endpoint nor the time.
    if !parsed.signed_headers.iter().any(|h| h == "host") {
        return Err(S3Error::BadRequest(
            "SignedHeaders must include host".into(),
        ));
    }
    if !parsed.signed_headers.iter().any(|h| h == "x-amz-date") {
        return Err(S3Error::BadRequest(
            "SignedHeaders must include x-amz-date".into(),
        ));
    }

    // Payload hash: clients either send x-amz-content-sha256 or mark the
    // payload unsigned. A declared hash must match the received bytes.
    let payload_hash = headers
        .get("x-amz-content-sha256")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("UNSIGNED-PAYLOAD");
    if payload_hash != "UNSIGNED-PAYLOAD" && !payload_hash.starts_with("STREAMING") {
        let actual = sha256_hex(body);
        if !str_eq_ct(&actual, payload_hash) {
            return Err(S3Error::SignatureMismatch);
        }
    }

    let canonical_uri = canonical_uri(req.uri.path());
    let canonical_query = canonical_query_string(req.uri.query());
    let canonical_headers = canonical_headers(&parsed.signed_headers, headers)?;

    verify(
        &parsed,
        req.method.as_str(),
        &canonical_uri,
        &canonical_query,
        &canonical_headers,
        payload_hash,
        &creds.secret,
        amz_date,
    )
}

/// Constant-time string equality (both sides lowercase hex / ASCII).
fn str_eq_ct(a: &str, b: &str) -> bool {
    use subtle::ConstantTimeEq;
    if a.len() != b.len() {
        return false;
    }
    a.as_bytes().ct_eq(b.as_bytes()).into()
}

/// AWS canonical URI: percent-encode every segment except unreserved
/// characters and the path separator. Raw `req.uri().path()` used to be
/// signed as-is, so an unencoded key could verify differently than AWS
/// clients sign it.
fn canonical_uri(path: &str) -> String {
    if path.is_empty() {
        return "/".to_string();
    }
    let mut out = String::with_capacity(path.len() + 8);
    for segment in path.split('/') {
        if segment.is_empty() {
            // leading (or duplicated) separator — the slash is emitted with
            // the next non-empty segment.
            continue;
        }
        out.push('/');
        aws_uri_encode(segment, &mut out);
    }
    if path.ends_with('/') {
        out.push('/');
    }
    out
}

/// Appends `s` to `out` with AWS-style URI encoding: unreserved characters
/// (`A-Z a-z 0-9 - _ . ~`) and `/` pass through (the caller splits on `/`),
/// everything else becomes `%XX`.
fn aws_uri_encode(s: &str, out: &mut String) {
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
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
///
/// R-07: a signed header that is absent from the request is an error, not a
/// silent skip — dropping it would let the verifier accept signatures that
/// never bound to the request actually sent.
fn canonical_headers(
    signed: &[String],
    headers: &axum::http::HeaderMap,
) -> Result<String, S3Error> {
    let mut out = String::new();
    for name in signed {
        let lower = name.to_lowercase();
        let Some(v) = headers.get(&lower).and_then(|v| v.to_str().ok()) else {
            return Err(S3Error::BadRequest(format!(
                "signed header '{lower}' missing from request"
            )));
        };
        out.push_str(&lower);
        out.push(':');
        out.push_str(v.trim());
        out.push('\n');
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sigv4::{canonical_request, derive_signing_key, sign, string_to_sign};
    use axum::http::Request;

    fn now_date() -> (String, String) {
        let now = time::OffsetDateTime::now_utc();
        let date = format!(
            "{:04}{:02}{:02}",
            now.year(),
            u8::from(now.month()),
            now.day()
        );
        let amz_date = format!(
            "{:02}{:02}{:02}T{:02}{:02}{:02}Z",
            now.year(),
            u8::from(now.month()),
            now.day(),
            now.hour(),
            now.minute(),
            now.second()
        );
        (date, amz_date)
    }

    /// Builds a signed request with our own primitives.
    fn signed_request(
        creds: &S3Credential,
        method: &str,
        uri: &str,
        query: Option<&str>,
        payload_hash: &str,
        date: &str,
        amz_date: &str,
        signed_headers: &[&str],
        body: &[u8],
    ) -> (Parts, Vec<u8>) {
        let names: Vec<String> = signed_headers.iter().map(|s| (*s).to_string()).collect();
        let mut builder = Request::builder()
            .method(method)
            .uri(format!(
                "{uri}{}",
                query.map(|q| format!("?{q}")).unwrap_or_default()
            ))
            .header("host", "localhost:8080")
            .header("x-amz-date", amz_date);
        if payload_hash != "UNSIGNED-PAYLOAD" {
            builder = builder.header("x-amz-content-sha256", payload_hash);
        }
        // Every signed header must exist on the request.
        for name in signed_headers {
            if *name != "host" && *name != "x-amz-date" {
                builder = builder.header(*name, axum::http::HeaderValue::from_static("x"));
            }
        }
        let canonical_headers = {
            let mut out = String::new();
            for name in &names {
                let v = if name == "host" {
                    "localhost:8080".to_string()
                } else if name == "x-amz-date" {
                    amz_date.to_string()
                } else {
                    "x".to_string()
                };
                out.push_str(&format!("{name}:{v}\n"));
            }
            out
        };
        let _ = &names;
        let region = "us-east-1";
        let date_scope = format!("{date}/{region}/s3/aws4_request");
        let canonical = canonical_request(
            method,
            &canonical_uri(uri),
            &canonical_query_string(query),
            &canonical_headers,
            &signed_headers.join(";"),
            payload_hash,
        );
        let s2s = string_to_sign("AWS4-HMAC-SHA256", amz_date, &date_scope, &canonical);
        let key = derive_signing_key(&creds.secret, date, region, "s3");
        let signature = sign(&key, &s2s);
        let authz = format!(
            "AWS4-HMAC-SHA256 Credential={}/{}/{}/{}/aws4_request, SignedHeaders={}, Signature={}",
            creds.access_key,
            date,
            region,
            "s3",
            names.join(";"),
            signature
        );
        let req = builder.header("authorization", authz).body(()).unwrap();
        let (parts, ()) = req.into_parts();
        (parts, body.to_vec())
    }

    fn creds() -> S3Credential {
        S3Credential {
            access_key: "AKIDTEST".into(),
            secret: "s3cr3t".into(),
        }
    }

    #[test]
    fn verify_request_accepts_well_signed_and_rejects_tampered() {
        let c = creds();
        let (date, amz_date) = now_date();
        let (parts, body) = signed_request(
            &c,
            "GET",
            "/picroom/test.bin",
            None,
            "UNSIGNED-PAYLOAD",
            &date,
            &amz_date,
            &["host", "x-amz-date"],
            b"",
        );
        assert!(
            verify_request(&parts, &body, &c).is_ok(),
            "well-signed request must verify"
        );

        // Tamper: replace the access key.
        let mut bad_creds = c;
        bad_creds.access_key = "OTHER".into();
        assert!(matches!(
            verify_request(&parts, &body, &bad_creds),
            Err(S3Error::SignatureMismatch)
        ));
    }

    /// R-03: a signature dated 2020 (outside the 15-minute window) must be
    /// rejected — captured Authorization headers cannot be replayed forever.
    #[test]
    fn stale_signature_is_rejected() {
        let c = creds();
        let (parts, body) = signed_request(
            &c,
            "GET",
            "/picroom/test.bin",
            None,
            "UNSIGNED-PAYLOAD",
            "20200101",
            "20200101T000000Z",
            &["host", "x-amz-date"],
            b"",
        );
        assert!(
            verify_request(&parts, &body, &c).is_err(),
            "a 2020-dated signature must be rejected"
        );
    }

    /// R-07: when a payload hash is declared, a swapped body must be caught.
    #[test]
    fn body_swap_is_rejected() {
        let c = creds();
        let (date, amz_date) = now_date();
        let real = sha256_hex(b"innocent");
        let (parts, body) = signed_request(
            &c,
            "PUT",
            "/b/k.png",
            None,
            &real,
            &date,
            &amz_date,
            &["host", "x-amz-date"],
            b"innocent",
        );
        assert!(verify_request(&parts, &body, &c).is_ok());

        // Same signature, different bytes → reject.
        let swapped = b"malicious".to_vec();
        assert!(matches!(
            verify_request(&parts, &swapped, &c),
            Err(S3Error::SignatureMismatch)
        ));
    }

    /// R-07: `SignedHeaders` without `host` binds to nothing — reject.
    #[test]
    fn signed_headers_missing_host_is_rejected() {
        let c = creds();
        let (date, amz_date) = now_date();
        let (parts, body) = signed_request(
            &c,
            "GET",
            "/b/k.png",
            None,
            "UNSIGNED-PAYLOAD",
            &date,
            &amz_date,
            &["x-amz-date"],
            b"",
        );
        assert!(matches!(
            verify_request(&parts, &body, &c),
            Err(S3Error::BadRequest(_))
        ));
    }

    /// R-07: a declared header absent from the request is an error.
    #[test]
    fn missing_declared_header_is_rejected() {
        let c = creds();
        let (date, amz_date) = now_date();
        // Sign content-type but do not send it.
        let names = [
            "host".to_string(),
            "x-amz-date".to_string(),
            "content-type".to_string(),
        ];
        let canonical_headers =
            format!("host:localhost:8080\nx-amz-date:{amz_date}\ncontent-type:image/png\n");
        let date_scope = format!("{date}/us-east-1/s3/aws4_request");
        let canonical = canonical_request(
            "PUT",
            "/b/k.png",
            "",
            &canonical_headers,
            &names.join(";"),
            "UNSIGNED-PAYLOAD",
        );
        let s2s = string_to_sign("AWS4-HMAC-SHA256", &amz_date, &date_scope, &canonical);
        let key = derive_signing_key(&c.secret, &date, "us-east-1", "s3");
        let signature = sign(&key, &s2s);
        let authz = format!(
            "AWS4-HMAC-SHA256 Credential={}/{}/us-east-1/s3/aws4_request, SignedHeaders={}, Signature={}",
            c.access_key, date, names.join(";"), signature
        );
        let req = Request::builder()
            .method("PUT")
            .uri("/b/k.png")
            .header("host", "localhost:8080")
            .header("x-amz-date", &amz_date)
            .header("authorization", authz)
            .body(())
            .unwrap();
        let (parts, ()) = req.into_parts();
        assert!(matches!(
            verify_request(&parts, b"", &c),
            Err(S3Error::BadRequest(_))
        ));
    }

    #[test]
    fn verify_request_rejects_wrong_access_key() {
        let c = creds();
        let authz = "AWS4-HMAC-SHA256 Credential=OTHERKEY/20240101/us-east-1/s3/aws4_request, SignedHeaders=host, Signature=abc";
        let req = Request::builder()
            .method("GET")
            .uri("/")
            .header("host", "localhost")
            .header("x-amz-date", "20240101T000000Z")
            .header("authorization", authz)
            .body(())
            .unwrap();
        let (parts, ()) = req.into_parts();
        assert!(matches!(
            verify_request(&parts, b"", &c),
            Err(S3Error::SignatureMismatch)
        ));
    }

    #[test]
    fn canonical_uri_encodes_reserved_characters() {
        assert_eq!(canonical_uri("/b/a b.png"), "/b/a%20b.png");
        assert_eq!(canonical_uri("/b/k(1).png"), "/b/k%281%29.png");
        assert_eq!(canonical_uri("/b/plain.png"), "/b/plain.png");
    }
}
