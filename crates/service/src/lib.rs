// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Picroom Contributors

//! # Picroom Service
//!
//! Use-case orchestration layer.

#![allow(missing_docs)]

pub mod authz;
pub mod delete;
pub mod error;
pub mod permission;
pub mod query;
pub mod quota;
pub mod repo;
pub mod upload;

pub use authz::AuthzService;
pub use delete::DeleteService;
pub use error::ServiceError;
pub use permission::PermissionService;
pub use query::ImageQueryService;
pub use quota::QuotaService;
pub use repo::{
    AclGrant, ImageRepository, PgImageRepository, PgResourceAclRepository, PgSessionRepository,
    PgStoragePolicyRepository, PgTeamRepository, PgUserRepository, PgVariantRepository,
    ResourceAclRepository, SessionRepository, SessionRow, SqliteResourceAclRepository,
    StoragePolicy, StoragePolicyRepository, TeamRepository, UserCredentials, UserRepository,
};
pub use upload::UploadService;
