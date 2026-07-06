//! # Picroom Storage
//!
//! Storage abstraction layer with trait-based drivers for multiple backends.
//!
//! The traits are split per Interface Segregation Principle (see ADR-0003):
//!
//! - [`StorageReader`]: read operations
//! - [`StorageWriter`]: write operations
//! - [`StorageLister`]: listing operations
//! - [`StorageSigner`]: URL signing (cloud-only)
//! - [`Storage`]: supertrait combining all four
//!
//! Concrete drivers implement these traits; an [`AnyStorage`] enum provides
//! zero-cost static dispatch over all drivers.

#![warn(missing_docs)]

pub mod any;
pub mod contract_test;
pub mod driver;
pub mod error;
pub mod signing;

pub use any::AnyStorage;
pub use driver::{ObjectMeta, Storage, StorageLister, StorageReader, StorageSigner, StorageWriter};
pub use error::StorageError;
