// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Picroom Contributors

//! # Picroom Auth
//!
//! Authentication, authorization, and RBAC.
//!
//! - [`password`]: Argon2id hashing
//! - [`jwt`]: JWT issuing and verification
//! - [`oidc`]: `OpenID` Connect integration
//! - [`rbac`]: role-based access control engine

#![allow(missing_docs)]

pub mod jwt;
pub mod oidc;
pub mod password;
pub mod rbac;

pub use jwt::{JwtClaims, JwtService};
pub use oidc::{
    verify_id_token, HttpOidcClient, IdTokenClaims, OidcClient, OidcError, OidcProvider,
    OidcUserInfo,
};
pub use password::{PasswordError, PasswordHasher};
pub use rbac::{
    AclEffect, AclRule, AclSubject, Actor, Decision, Permission, PermissionAction, RbacEngine,
    Resource, ResourceType, Role,
};
