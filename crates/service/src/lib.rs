//! # Picroom Service
//!
//! Use-case orchestration layer.

#![warn(missing_docs)]

pub mod delete;
pub mod error;
pub mod permission;
pub mod query;
pub mod quota;
pub mod repo;
pub mod upload;

pub use delete::DeleteService;
pub use error::ServiceError;
pub use permission::PermissionService;
pub use query::ImageQueryService;
pub use quota::QuotaService;
pub use repo::{ImageRepository, PgImageRepository, PgVariantRepository};
pub use upload::UploadService;
