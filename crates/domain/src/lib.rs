// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Picroom Contributors

//! # Picroom Domain
//!
//! Pure domain types, value objects, traits, and errors for Picroom.
//!
//! This crate contains **no I/O**. It is the innermost layer of the
//! architecture and depends only on `std`, `thiserror`, and (optionally)
//! `serde`.
//!
//! ## Modules
//!
//! - [`error`]: domain error types
//! - [`image`]: image entity
//! - [`user`]: user entity
//! - [`team`]: team entity
//! - [`role`]: role definitions
//! - [`permission`]: permission enums + check functions
//! - [`storage_key`]: validated storage key
//! - [`page`]: pagination primitives
//! - [`clock`]: clock trait + system/fake impls
//! - [`id`]: UUID v7 generator
//!
//! See [`docs/spec.md`](https://github.com/picroom/picroom/blob/main/docs/spec.md)
//! for the full domain specification.

#![allow(missing_docs)]

pub mod clock;
pub mod error;
pub mod id;
pub mod image;
pub mod page;
pub mod permission;
pub mod role;
pub mod storage_key;
pub mod team;
pub mod user;

pub use clock::{Clock, SystemClock};
pub use error::DomainError;
pub use image::{Image, ImageId, ImageVariant};
pub use page::{Page, PageReq};
pub use permission::{Permission, PermissionAction, ResourceType};
pub use role::{Role, RolePermissions};
pub use storage_key::StorageKey;
pub use team::{Team, TeamId, TeamMember};
pub use user::{NewUser, User, UserId};
