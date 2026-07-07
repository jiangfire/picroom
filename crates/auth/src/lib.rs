//! # Picroom Auth
//!
//! Authentication, authorization, and RBAC.
//!
//! - [`password`]: Argon2id hashing
//! - [`jwt`]: JWT issuing and verification
//! - [`api_token`]: long-lived bearer tokens
//! - [`oidc`]: `OpenID` Connect integration
//! - [`rbac`]: role-based access control engine

#![allow(missing_docs)]

pub mod api_token;
pub mod jwt;
pub mod oidc;
pub mod password;
pub mod rbac;

pub use api_token::{ApiToken, ApiTokenService};
pub use jwt::{JwtClaims, JwtService};
pub use oidc::{OidcError, OidcProvider};
pub use password::{PasswordError, PasswordHasher};
pub use rbac::{Decision, Permission, PermissionAction, RbacEngine, Resource, ResourceType, Role};
