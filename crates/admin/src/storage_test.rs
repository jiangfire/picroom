//! Storage round-trip test subcommand (skeleton).

use bytes::Bytes;
use picroom_domain::StorageKey;
use picroom_storage::{Storage, StorageReader, StorageWriter};
use thiserror::Error;

/// Storage-test errors.
#[derive(Debug, Error)]
pub enum StorageTestError {
    /// Storage failed.
    #[error("storage: {0}")]
    Storage(String),
}

/// Performs a put-get-delete round-trip on the given driver.
pub async fn storage_test<S: Storage>(driver: &S) -> Result<(), StorageTestError> {
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
