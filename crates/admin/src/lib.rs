//! # Picroom Admin
//!
//! CLI subcommands: migrate, user, team, audit, config, storage-test.

#![allow(missing_docs)]

pub mod audit_cmd;
pub mod config_cmd;
pub mod migrate;
pub mod storage_test;
pub mod team;
pub mod user;

pub use audit_cmd::audit_tail;
pub use config_cmd::{config_print, config_validate};
pub use migrate::migrate_run;
pub use storage_test::storage_test;
pub use team::{team_add_member_sqlite, team_create_sqlite, team_list_sqlite, TeamCmd};
pub use user::{
    user_create_sqlite, user_disable_sqlite, user_list_sqlite, user_set_role_sqlite, UserCmd,
};
