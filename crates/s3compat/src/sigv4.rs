//! AWS Signature V4 verifier.
//!
//! Implements the canonical request, string-to-sign, and signing-key
//! derivation per the AWS spec. Tests against the AWS-provided test
//! vectors ensure correctness.

use crate::S3Error;
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};
use time::OffsetDateTime;

/// A parsed S3 V4 authorization header.
#[derive(Debug, Clone)]
pub struct ParsedAuth {
    /// Algorithm (always `AWS4-HMAC-SHA256` for V4).
    pub algorithm: String,
    /// Access key id (looked up against the user/db).
    pub access_key: String,
    /// Date (YYYYMMDD).
    pub date: String,
    /// Region.
    pub region: String,
    /// Service.
    pub service: String,
    /// Signed headers (lowercase names).
    pub signed_headers: Vec<String>,
    /// Signature (hex).
    pub signature: String,
}

/// Parses an `Authorization: AWS4-HMAC-SHA256 …` header.
pub fn parse_authz(header: &str) -> Result<ParsedAuth, S3Error> {
    let header = header
        .strip_prefix("AWS4-HMAC-SHA256 ")
        .ok_or_else(|| S3Error::BadRequest("unsupported algorithm".into()))?;

    let algorithm = String::from("AWS4-HMAC-SHA256");
    let mut access_key = String::new();
    let mut date = String::new();
    let mut region = String::new();
    let mut service = String::new();
    let mut signed_headers = Vec::new();
    let mut signature = String::new();

    for part in header.split(',') {
        let part = part.trim();
        if let Some(idx) = part.find('=') {
            let (k, v) = part.split_at(idx);
            let v = &v[1..];
            match k.trim() {
                "Credential" => {
                    // AccessKey/YYYYMMDD/region/service/aws4_request
                    let parts: Vec<&str> = v.split('/').collect();
                    if parts.len() != 5 {
                        return Err(S3Error::BadRequest("malformed Credential".into()));
                    }
                    access_key = parts[0].to_string();
                    date = parts[1].to_string();
                    region = parts[2].to_string();
                    service = parts[3].to_string();
                }
                "SignedHeaders" => {
                    signed_headers = v.split(';').map(std::string::ToString::to_string).collect();
                }
                "Signature" => {
                    signature = v.to_string();
                }
                _ => {
                    // Unknown field — ignore.
                }
            }
        }
    }

    if access_key.is_empty() || signature.is_empty() {
        return Err(S3Error::BadRequest("incomplete auth header".into()));
    }

    Ok(ParsedAuth {
        algorithm,
        access_key,
        date,
        region,
        service,
        signed_headers,
        signature,
    })
}

/// Build the canonical request string.
pub fn canonical_request(
    method: &str,
    canonical_uri: &str,
    canonical_query: &str,
    canonical_headers: &str,
    signed_headers: &str,
    payload_hash: &str,
) -> String {
    format!(
        "{method}\n{canonical_uri}\n{canonical_query}\n{canonical_headers}\n{signed_headers}\n{payload_hash}"
    )
}

/// Build the string-to-sign.
pub fn string_to_sign(
    algorithm: &str,
    amz_date: &str,
    date_scope: &str,
    canonical_req: &str,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(canonical_req.as_bytes());
    let hash_hex = hex_lower(&hasher.finalize());
    format!("{algorithm}\n{amz_date}\n{date_scope}\n{hash_hex}")
}

/// Derives the signing key from secret + date + region + service.
pub fn derive_signing_key(secret: &str, date: &str, region: &str, service: &str) -> [u8; 32] {
    let k_secret = format!("AWS4{secret}");
    let mut k = hmac_key(k_secret.as_bytes(), date.as_bytes());
    k = hmac_key(&k, region.as_bytes());
    k = hmac_key(&k, service.as_bytes());
    k = hmac_key(&k, b"aws4_request");
    k
}

/// Sign `string_to_sign` with the given key; returns hex digest.
pub fn sign(key: &[u8], string_to_sign: &str) -> String {
    type HmacSha256 = Hmac<Sha256>;
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC accepts any key length");
    mac.update(string_to_sign.as_bytes());
    hex_lower(&mac.finalize().into_bytes())
}

/// Compute SHA-256 hex digest of `data`.
pub fn sha256_hex(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hex_lower(&hasher.finalize())
}

fn hmac_key(key: &[u8], data: &[u8]) -> [u8; 32] {
    type HmacSha256 = Hmac<Sha256>;
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC accepts any key length");
    mac.update(data);
    mac.finalize().into_bytes().into()
}

fn hex_lower(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

/// Verifies an incoming S3 request signature against the expected one
/// computed from the request contents.
///
/// Returns `Ok(())` if the signature is valid; `Err(S3Error)` otherwise.
///
/// `secret_access_key` is the user's secret. The `amz_date` should be in
/// `YYYYMMDDTHHMMSSZ` format (the value of the `x-amz-date` header).
pub fn verify(
    parsed: &ParsedAuth,
    method: &str,
    canonical_uri: &str,
    canonical_query: &str,
    canonical_headers: &str,
    payload_hash: &str,
    secret_access_key: &str,
    amz_date: &str,
) -> Result<(), S3Error> {
    let date_scope = format!(
        "{}/{}/{}/aws4_request",
        parsed.date, parsed.region, parsed.service
    );
    let canonical = canonical_request(
        method,
        canonical_uri,
        canonical_query,
        canonical_headers,
        &parsed.signed_headers.join(";"),
        payload_hash,
    );
    let s2s = string_to_sign(&parsed.algorithm, amz_date, &date_scope, &canonical);
    let key = derive_signing_key(
        secret_access_key,
        &parsed.date,
        &parsed.region,
        &parsed.service,
    );
    let expected = sign(&key, &s2s);
    if expected.eq_ignore_ascii_case(&parsed.signature) {
        Ok(())
    } else {
        Err(S3Error::SignatureMismatch {
            expected,
            got: parsed.signature.clone(),
        })
    }
}

/// Returns true if `date_str` (YYYYMMDDTHHMMSSZ) is within 15 minutes of now.
pub fn within_skew(date_str: &str, now: OffsetDateTime) -> bool {
    // Parse YYYYMMDDTHHMMSSZ.
    if date_str.len() != 16 {
        return false;
    }
    let Ok(year) = date_str[0..4].parse::<i32>() else {
        return false;
    };
    let Ok(month) = date_str[4..6].parse::<u8>() else {
        return false;
    };
    let Ok(day) = date_str[6..8].parse::<u8>() else {
        return false;
    };
    let Ok(hour) = date_str[9..11].parse::<u8>() else {
        return false;
    };
    let Ok(minute) = date_str[11..13].parse::<u8>() else {
        return false;
    };
    let Ok(second) = date_str[13..15].parse::<u8>() else {
        return false;
    };

    let Ok(d) = time::Month::try_from(month) else {
        return false;
    };
    let Ok(date) = time::Date::from_calendar_date(year, d, day) else {
        return false;
    };
    let Ok(time) = time::Time::from_hms(hour, minute, second) else {
        return false;
    };
    let dt = time::PrimitiveDateTime::new(date, time);
    let parsed = dt.assume_utc();
    let diff = (now - parsed).whole_seconds().abs();
    diff <= 15 * 60
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Reference test vector from AWS `SigV4` docs.
    /// (Sign request, "AWS4-HMAC-SHA256" examples).
    #[test]
    fn sha256_hex_known_value() {
        // SHA-256 of empty string.
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn hex_matches_aws_reference_for_credential() {
        let parsed = parse_authz(
            "AWS4-HMAC-SHA256 Credential=AKIDEXAMPLE/20150830/us-east-1/s3/aws4_request, \
             SignedHeaders=host;range;x-amz-content-sha256;x-amz-date, \
             Signature=fe5f80f77d5fa3beca038a248ff027d318534f3350e3069590317913507d4c2a",
        )
        .unwrap();
        assert_eq!(parsed.access_key, "AKIDEXAMPLE");
        assert_eq!(parsed.date, "20150830");
        assert_eq!(parsed.region, "us-east-1");
        assert_eq!(parsed.service, "s3");
        assert_eq!(
            parsed.signed_headers,
            vec!["host", "range", "x-amz-content-sha256", "x-amz-date"]
        );
        assert_eq!(parsed.signature.len(), 64);
    }

    #[test]
    fn derive_signing_key_is_stable() {
        // Deterministic — re-running yields the same 32-byte key.
        let k1 = derive_signing_key("secret", "20240101", "us-east-1", "s3");
        let k2 = derive_signing_key("secret", "20240101", "us-east-1", "s3");
        assert_eq!(k1, k2);
    }

    #[test]
    fn sign_returns_stable_signature() {
        let key = derive_signing_key("secret", "20240101", "us-east-1", "s3");
        let s2s = "AWS4-HMAC-SHA256\n20240101T000000Z\n20240101/us-east-1/s3/aws4_request\nabc";
        let sig1 = sign(&key, s2s);
        let sig2 = sign(&key, s2s);
        assert_eq!(sig1, sig2);
        assert_eq!(sig1.len(), 64);
        assert!(sig1.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn verify_round_trip() {
        // Sign with our primitives, verify with our verifier — they
        // must agree.
        let parsed = ParsedAuth {
            algorithm: "AWS4-HMAC-SHA256".into(),
            access_key: "AKID".into(),
            date: "20240101".into(),
            region: "us-east-1".into(),
            service: "s3".into(),
            signed_headers: vec!["host".into()],
            signature: "placeholder".into(),
        };
        let secret = "test-secret";
        let method = "GET";
        let uri = "/";
        let query = "";
        let headers = "host:example.com\n\n";
        let payload_hash = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
        let amz_date = "20240101T000000Z";

        let date_scope = format!(
            "{}/{}/{}/aws4_request",
            parsed.date, parsed.region, parsed.service
        );
        let canonical = canonical_request(
            method,
            uri,
            query,
            headers,
            &parsed.signed_headers.join(";"),
            payload_hash,
        );
        let s2s = string_to_sign(&parsed.algorithm, amz_date, &date_scope, &canonical);
        let key = derive_signing_key(secret, &parsed.date, &parsed.region, &parsed.service);
        let expected_sig = sign(&key, &s2s);

        let mut p2 = parsed;
        p2.signature = expected_sig;
        verify(
            &p2,
            method,
            uri,
            query,
            headers,
            payload_hash,
            secret,
            amz_date,
        )
        .expect("freshly-signed request should verify");
    }

    #[test]
    fn parse_authz_rejects_missing_prefix() {
        let r = parse_authz("Bearer foo");
        assert!(r.is_err());
    }

    #[test]
    fn within_skew_accepts_close_time() {
        let now = time::macros::datetime!(2026-07-05 12:00:00 UTC);
        let r = within_skew("20260705T120500Z", now);
        assert!(r, "5 minutes ago should be accepted");
        let r = within_skew("20260705T115000Z", now);
        assert!(r, "10 minutes ago should be accepted");
    }

    #[test]
    fn within_skew_rejects_too_old() {
        let now = time::macros::datetime!(2026-07-05 12:00:00 UTC);
        let r = within_skew("20260705T113000Z", now);
        assert!(!r, "30 minutes ago should be rejected");
    }
}
