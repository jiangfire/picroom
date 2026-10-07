// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Picroom Contributors

//! Route/OpenAPI drift check (R-24, task 3.4).
//!
//! Every route registered in `router.rs` must have a `paths` entry in
//! `docs/api/openapi.yaml`, and every `/api/v1` + `/i` path in the document
//! must actually be registered — in both directions, so adding a route
//! without a spec entry (or deleting one while the spec lingers) fails CI.

use std::collections::BTreeSet;
use std::path::PathBuf;

const ROUTER_RS: &str = "src/router.rs";
const OPENAPI_YAML: &str = "../../docs/api/openapi.yaml";

/// Reads a workspace file relative to this crate.
fn read_workspace_file(rel: &str) -> String {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest.join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
}

/// `:param` -> `{param}`; `*rest` -> `{rest}`.
fn normalize_path(raw: &str) -> String {
    let mut out = String::new();
    for segment in raw.split('/') {
        if segment.is_empty() {
            continue;
        }
        out.push('/');
        if let Some(name) = segment.strip_prefix(':') {
            out.push_str(&format!("{{{name}}}"));
        } else if let Some(name) = segment.strip_prefix('*') {
            out.push_str(&format!("{{{name}}}"));
        } else {
            out.push_str(segment);
        }
    }
    out
}

/// Collapses parameter placeholders so path *shapes* compare equal even when
/// the doc and the code name the parameter differently (`{teamId}` vs `{id}`).
fn normalize_shape(path: &str) -> String {
    let mut out = String::new();
    let mut in_param = false;
    for ch in path.chars() {
        match ch {
            '{' => {
                in_param = true;
                out.push('{');
            }
            '}' => {
                in_param = false;
                out.push('}');
            }
            _c if in_param => {}
            c => out.push(c),
        }
    }
    out
}

/// Extracts the route paths registered in `router.rs` from `.route("...")`
/// and `.nest("...")` calls — the literal may sit on the following line.
fn registered_routes(router_source: &str) -> BTreeSet<String> {
    let mut routes = BTreeSet::new();
    let bytes = router_source.as_bytes();
    let mut idx = 0;
    while let Some(pos) = router_source[idx..]
        .find(".route(")
        .or_else(|| router_source[idx..].find(".nest("))
    {
        let start = idx + pos;
        idx = start + 1;
        // Scan forward past whitespace and line comments to the first quote.
        let mut cursor = start;
        while cursor < bytes.len() {
            let b = bytes[cursor];
            if b == b'"' {
                break;
            }
            if b == b'/' && cursor + 1 < bytes.len() && bytes[cursor + 1] == b'/' {
                while cursor < bytes.len() && bytes[cursor] != b'\n' {
                    cursor += 1;
                }
                continue;
            }
            cursor += 1;
        }
        if cursor >= bytes.len() || bytes[cursor] != b'"' {
            continue;
        }
        let rest = &router_source[cursor + 1..];
        let Some(end) = rest.find('"') else { continue };
        let normalized = normalize_shape(&normalize_path(&rest[..end]));
        if !normalized.is_empty() {
            routes.insert(normalized);
        }
    }
    routes
}

/// Extracts the top-level `paths:` keys from the `OpenAPI` document.
fn documented_paths(yaml: &str) -> BTreeSet<String> {
    let value: serde_yaml::Value = serde_yaml::from_str(yaml).expect("openapi.yaml must parse");
    let mut paths = BTreeSet::new();
    if let Some(map) = value.get("paths").and_then(|p| p.as_mapping()) {
        for key in map.keys() {
            if let Some(path) = key.as_str() {
                paths.insert(path.to_string());
            }
        }
    }
    paths
}

#[test]
fn every_registered_route_is_documented() {
    let routes = registered_routes(&read_workspace_file(ROUTER_RS));
    let docs: BTreeSet<String> = documented_paths(&read_workspace_file(OPENAPI_YAML))
        .iter()
        .map(|p| normalize_shape(p))
        .collect();

    // The `/s3` nest prefix is a mount point, not a route (its sub-routes
    // are documented as `/s3/{bucket}` and `/s3/{bucket}/{key}`).
    let missing: Vec<&String> = routes
        .iter()
        .filter(|r| !docs.contains(*r) && r.as_str() != "/s3")
        .collect();
    assert!(
        missing.is_empty(),
        "routes registered in router.rs but missing from openapi.yaml: {missing:?}"
    );
}

#[test]
fn every_documented_api_path_is_registered() {
    let routes = registered_routes(&read_workspace_file(ROUTER_RS));
    let docs = documented_paths(&read_workspace_file(OPENAPI_YAML));

    // Scope the reverse check to the API surface this crate owns. `/s3/*`
    // sub-routes are mounted from the s3compat crate (documented under
    // `/s3/...`), and the `/s3` nest prefix itself is intentionally not a
    // path entry.
    let stale: Vec<&String> = docs
        .iter()
        .filter(|p| {
            let shaped = normalize_shape(p);
            (p.starts_with("/api/v1") || p.starts_with("/i/")) && !routes.contains(&shaped)
        })
        .collect();
    assert!(
        stale.is_empty(),
        "openapi.yaml documents paths that router.rs does not register: {stale:?}"
    );
}

#[test]
fn normalisation_matches_openapi_syntax() {
    assert_eq!(
        normalize_path("/api/v1/images/:id/acl"),
        "/api/v1/images/{id}/acl"
    );
    assert_eq!(normalize_path("/i/*key"), "/i/{key}");
    assert_eq!(normalize_path("/healthz"), "/healthz");
    assert_eq!(
        normalize_shape("/api/v1/images/{imageId}/acl"),
        normalize_shape("/api/v1/images/{id}/acl")
    );
}
