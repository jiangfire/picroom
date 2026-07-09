// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Picroom Contributors

//! Local filesystem driver.
//!
//! Stores objects under a configurable root directory. Each object is
//! written atomically via `temp + rename` to avoid partial reads.

use crate::driver::{StorageLister, StorageReader, StorageSigner, StorageWriter};
use crate::StorageError;
use async_trait::async_trait;
use bytes::Bytes;
use picroom_domain::StorageKey;
use std::path::{Path, PathBuf};
use std::time::Duration;
use time::OffsetDateTime;
use tokio::fs;
use url::Url;

/// Filesystem-backed storage driver.
#[derive(Debug, Clone)]
pub struct LocalDriver {
    root: PathBuf,
    url_prefix: String,
}

impl LocalDriver {
    /// Creates a new local driver rooted at `root`.
    pub fn new(root: PathBuf, url_prefix: impl Into<String>) -> Self {
        Self {
            root,
            url_prefix: url_prefix.into(),
        }
    }

    /// Returns the configured root directory.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Resolves a storage key to an absolute filesystem path with safety check.
    pub fn resolve(&self, key: &StorageKey) -> Result<PathBuf, StorageError> {
        let path = self.root.join(key.as_str());
        // Defence-in-depth: reject any path that escapes the root.
        if let Ok(canonical) = path.canonicalize() {
            let canonical_root = self
                .root
                .canonicalize()
                .unwrap_or_else(|_| self.root.clone());
            if !canonical.starts_with(&canonical_root) {
                return Err(StorageError::PermissionDenied(format!(
                    "path escapes root: {}",
                    key.as_str()
                )));
            }
        }
        Ok(path)
    }

    /// Resolves a key to a path *without* canonicalization (for writing).
    fn resolve_unchecked(&self, key: &StorageKey) -> Result<PathBuf, StorageError> {
        Ok(self.root.join(key.as_str()))
    }
}

#[async_trait]
impl StorageReader for LocalDriver {
    async fn get(&self, key: &StorageKey) -> Result<Bytes, StorageError> {
        let path = self.resolve(key)?;
        match fs::read(&path).await {
            Ok(bytes) => Ok(Bytes::from(bytes)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                Err(StorageError::NotFound(key.as_str().to_string()))
            }
            Err(e) => Err(StorageError::Backend(format!(
                "read {}: {e}",
                path.display()
            ))),
        }
    }

    async fn head(&self, key: &StorageKey) -> Result<crate::driver::ObjectMeta, StorageError> {
        let path = self.resolve(key)?;
        let meta = match fs::metadata(&path).await {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Err(StorageError::NotFound(key.as_str().to_string()));
            }
            Err(e) => {
                return Err(StorageError::Backend(format!(
                    "stat {}: {e}",
                    path.display()
                )));
            }
        };
        let modified = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map_or_else(OffsetDateTime::now_utc, |d| {
                OffsetDateTime::from_unix_timestamp(d.as_secs() as i64)
                    .unwrap_or_else(|_| OffsetDateTime::now_utc())
            });

        Ok(crate::driver::ObjectMeta {
            key: key.clone(),
            bytes: meta.len(),
            last_modified: modified,
            etag: None,
        })
    }

    async fn exists(&self, key: &StorageKey) -> Result<bool, StorageError> {
        let path = self.resolve_unchecked(key)?;
        Ok(fs::try_exists(&path).await.unwrap_or(false))
    }
}

#[async_trait]
impl StorageWriter for LocalDriver {
    async fn put(&self, key: &StorageKey, bytes: Bytes) -> Result<(), StorageError> {
        let path = self.resolve_unchecked(key)?;

        // Ensure parent directory exists.
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .await
                .map_err(|e| StorageError::Backend(format!("mkdir {}: {e}", parent.display())))?;
        }

        // Atomic write: temp file + rename.
        let mut tmp = path.clone();
        let tmp_name = format!(
            ".{}.tmp",
            path.file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("picroom")
        );
        tmp.set_file_name(tmp_name);

        fs::write(&tmp, &bytes)
            .await
            .map_err(|e| StorageError::Backend(format!("write tmp {}: {e}", tmp.display())))?;

        fs::rename(&tmp, &path)
            .await
            .map_err(|e| StorageError::Backend(format!("rename {}: {e}", path.display())))?;
        Ok(())
    }

    async fn delete(&self, key: &StorageKey) -> Result<(), StorageError> {
        let path = self.resolve_unchecked(key)?;
        match fs::remove_file(&path).await {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(StorageError::Backend(format!(
                "remove {}: {e}",
                path.display()
            ))),
        }
    }
}

#[async_trait]
impl StorageLister for LocalDriver {
    async fn list(
        &self,
        prefix: &StorageKey,
    ) -> Result<picroom_domain::Page<crate::driver::ObjectMeta>, StorageError> {
        let base = self.resolve_unchecked(prefix)?;
        if !fs::try_exists(&base).await.unwrap_or(false) {
            return Ok(picroom_domain::Page::new(
                vec![],
                None,
                picroom_domain::PageReq::default(),
            ));
        }
        let mut items = Vec::new();
        let count_hint;
        {
            collect_recursive(&base, &self.root, prefix, &mut items).await?;
            count_hint = items.len() as u32;
        }
        Ok(picroom_domain::Page::new(
            items,
            None,
            picroom_domain::PageReq {
                limit: count_hint,
                cursor: None,
            },
        ))
    }
}

async fn collect_recursive(
    base: &Path,
    root: &Path,
    prefix: &StorageKey,
    items: &mut Vec<crate::driver::ObjectMeta>,
) -> Result<(), StorageError> {
    let prefix_str = prefix.as_str();
    let mut stack = vec![base.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let mut rd = match fs::read_dir(&dir).await {
            Ok(r) => r,
            Err(_) => continue,
        };
        while let Ok(Some(entry)) = rd.next_entry().await {
            let p = entry.path();
            let meta = match entry.metadata().await {
                Ok(m) => m,
                Err(_) => continue,
            };
            if meta.is_dir() {
                stack.push(p);
                continue;
            }
            let rel = p.strip_prefix(root).unwrap_or(&p).to_path_buf();
            // Skip temp files (atomic-write leftovers).
            if rel
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with('.') && n.eq_ignore_ascii_case(".tmp"))
            {
                continue;
            }
            let key_str = rel.to_string_lossy().replace('\\', "/");
            let key = match StorageKey::parse(&key_str) {
                Ok(k) => k,
                Err(_) => continue,
            };
            // Filter by logical prefix.
            if !key.as_str().starts_with(prefix_str) {
                continue;
            }
            let modified = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map_or_else(OffsetDateTime::now_utc, |d| {
                    OffsetDateTime::from_unix_timestamp(d.as_secs() as i64)
                        .unwrap_or_else(|_| OffsetDateTime::now_utc())
                });

            items.push(crate::driver::ObjectMeta {
                key,
                bytes: meta.len(),
                last_modified: modified,
                etag: None,
            });
        }
    }
    Ok(())
}

#[async_trait]
impl StorageSigner for LocalDriver {
    async fn sign_get_url(&self, key: &StorageKey, _ttl: Duration) -> Result<Url, StorageError> {
        Ok(Url::parse(&format!(
            "{}/{}",
            self.url_prefix.trim_end_matches('/'),
            key.as_str()
        ))?)
    }

    async fn sign_put_url(&self, key: &StorageKey, _ttl: Duration) -> Result<Url, StorageError> {
        Ok(Url::parse(&format!(
            "{}/{}",
            self.url_prefix.trim_end_matches('/'),
            key.as_str()
        ))?)
    }
}

impl crate::driver::Storage for LocalDriver {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract_test::put_get_delete_roundtrip;

    #[test]
    fn new_stores_root_and_prefix() {
        let d = LocalDriver::new(PathBuf::from("/tmp/img"), "https://cdn.example.com/i");
        assert_eq!(d.root(), std::path::Path::new("/tmp/img"));
    }

    #[tokio::test]
    async fn roundtrip_passes_contract() {
        let tmp = tempdir();
        let d = LocalDriver::new(tmp.clone(), "/i");
        put_get_delete_roundtrip(&d).await.unwrap();
    }

    #[tokio::test]
    async fn put_then_get_returns_same_bytes() {
        let tmp = tempdir();
        let d = LocalDriver::new(tmp.clone(), "/i");
        let key = StorageKey::parse("x/y.bin").unwrap();
        d.put(&key, Bytes::from_static(b"hello")).await.unwrap();
        let got = d.get(&key).await.unwrap();
        assert_eq!(got, Bytes::from_static(b"hello"));
    }

    #[tokio::test]
    async fn get_missing_returns_not_found() {
        let tmp = tempdir();
        let d = LocalDriver::new(tmp.clone(), "/i");
        let key = StorageKey::parse("nope.bin").unwrap();
        let err = d.get(&key).await.unwrap_err();
        assert!(matches!(err, StorageError::NotFound(_)));
    }

    #[tokio::test]
    async fn delete_missing_succeeds() {
        let tmp = tempdir();
        let d = LocalDriver::new(tmp.clone(), "/i");
        let key = StorageKey::parse("nope.bin").unwrap();
        d.delete(&key).await.unwrap();
    }

    #[tokio::test]
    async fn head_returns_size() {
        let tmp = tempdir();
        let d = LocalDriver::new(tmp.clone(), "/i");
        let key = StorageKey::parse("h.bin").unwrap();
        d.put(&key, Bytes::from_static(b"abc")).await.unwrap();
        let meta = d.head(&key).await.unwrap();
        assert_eq!(meta.bytes, 3);
    }

    #[tokio::test]
    async fn sign_get_url_returns_prefixed_path() {
        let d = LocalDriver::new(PathBuf::from("/tmp"), "https://cdn.example.com/i");
        let key = StorageKey::parse("a/b.jpg").unwrap();
        let url = d.sign_get_url(&key, Duration::from_secs(60)).await.unwrap();
        assert_eq!(url.as_str(), "https://cdn.example.com/i/a/b.jpg");
    }

    fn tempdir() -> PathBuf {
        let base =
            std::env::temp_dir().join(format!("picroom-local-driver-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(&base).unwrap();
        base
    }
}
