//! Shared contract test for all storage drivers.
//!
//! Every driver implementation should run `put_get_delete_roundtrip` to
//! guarantee behavioural consistency.

use crate::driver::{Storage, StorageLister, StorageSigner};
use bytes::Bytes;
use picroom_domain::StorageKey;

/// Asserts a basic round-trip: put → get → delete → not-found.
pub async fn put_get_delete_roundtrip<D: Storage>(driver: &D) -> Result<(), crate::StorageError> {
    let key = StorageKey::parse("test/roundtrip.bin")
        .map_err(|e| crate::StorageError::Config(format!("key parse: {e}")))?;
    let payload = Bytes::from_static(b"hello world");

    driver.put(&key, payload.clone()).await?;
    let got = driver.get(&key).await?;
    assert_eq!(got, payload);

    let exists_before = driver.exists(&key).await?;
    assert!(exists_before, "object must exist after put");

    driver.delete(&key).await?;

    let exists_after = driver.exists(&key).await?;
    assert!(!exists_after, "object must not exist after delete");

    Ok(())
}

#[allow(dead_code)]
const fn _unused_signing_compiles<S: StorageSigner>(_s: &S) {}

#[allow(dead_code)]
const fn _unused_listing_compiles<S: StorageLister>(_s: &S) {}