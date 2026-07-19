// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Picroom Contributors

//! Storage round-trip test subcommand (skeleton).

use bytes::Bytes;
use picroom_domain::StorageKey;
use picroom_storage::Storage;
use thiserror::Error;

/// Storage-test errors.
#[derive(Debug, Error)]
pub enum StorageTestError {
    /// Storage failed.
    #[error("storage: {0}")]
    Storage(String),
}

/// Performs a put-get-delete round-trip on the given driver.
pub async fn storage_test<S: Storage + ?Sized>(driver: &S) -> Result<(), StorageTestError> {
    let key = StorageKey::parse("test/admin/roundtrip.bin")
        .map_err(|e| StorageTestError::Storage(e.to_string()))?;
    let payload = Bytes::from_static(b"picroom storage test");

    driver
        .put(&key, payload.clone())
        .await
        .map_err(|e| StorageTestError::Storage(e.to_string()))?;

    let got = driver
        .get(&key)
        .await
        .map_err(|e| StorageTestError::Storage(e.to_string()))?;
    if got != payload {
        return Err(StorageTestError::Storage("payload mismatch".into()));
    }

    driver
        .delete(&key)
        .await
        .map_err(|e| StorageTestError::Storage(e.to_string()))?;

    println!("storage test OK");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use picroom_storage::driver::local::LocalDriver;

    #[tokio::test]
    async fn roundtrip_against_local_driver() {
        let dir =
            std::env::temp_dir().join(format!("picroom-storage-test-{}", uuid::Uuid::now_v7()));
        let _ = std::fs::create_dir_all(&dir);
        let driver = LocalDriver::new(dir.clone(), "/i");
        storage_test(&driver)
            .await
            .expect("local round-trip should pass");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
