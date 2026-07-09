// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Picroom Contributors

//! Picroom — single binary entry point.
//!
//! Dispatches to `api`, `worker`, or `admin` subcommands.

use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::process::ExitCode;

mod api_cmd;
mod app;
mod banner;
mod shutdown;
mod worker_cmd;

#[derive(Parser, Debug)]
#[command(name = "picroom", version, about = "Self-hosted image hosting service")]
struct Cli {
    /// Path to TOML config file (overrides `PICROOM_CONFIG`).
    #[arg(long, env = "PICROOM_CONFIG", global = true)]
    config: Option<PathBuf>,

    /// Verbosity (-v, -vv, …).
    #[arg(short, long, action = clap::ArgAction::Count, global = true)]
    verbose: u8,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Run the HTTP API.
    Api {
        /// Override bind address (e.g. 0.0.0.0:9090).
        #[arg(long)]
        bind: Option<String>,
    },
    /// Run the async image-processing worker.
    Worker {
        /// Number of concurrent workers.
        #[arg(long, default_value_t = num_cpus())]
        concurrency: usize,
    },
    /// Administrative commands.
    #[command(subcommand)]
    Admin(AdminCmd),
    /// Print version and exit.
    Version,
}

/// Administrative subcommands.
#[derive(Subcommand, Debug)]
enum AdminCmd {
    /// Run database migrations.
    Migrate {
        #[command(subcommand)]
        action: MigrateAction,
    },
    /// Manage users.
    #[command(subcommand)]
    User(picroom_admin::UserCmd),
    /// Manage teams.
    #[command(subcommand)]
    Team(picroom_admin::TeamCmd),
    /// Tail the audit log.
    Audit {
        /// Follow new events as they arrive.
        #[arg(long, short)]
        follow: bool,
        /// Filter by actor email.
        #[arg(long)]
        actor: Option<String>,
    },
    /// Validate / print the configuration.
    #[command(subcommand)]
    Config(ConfigCmd),
    /// Round-trip test for a storage policy.
    StorageTest {
        /// Policy name from config.
        #[arg(long, default_value = "default")]
        policy: String,
    },
}

/// Migration actions.
#[derive(Subcommand, Debug)]
enum MigrateAction {
    /// Apply all pending migrations.
    Run,
    /// Revert the most recent migration.
    Revert,
    /// Show migration status.
    Status,
}

/// Config subcommands.
#[derive(Subcommand, Debug)]
enum ConfigCmd {
    /// Print resolved config as JSON.
    Print,
    /// Validate the loaded config.
    Validate,
}

fn num_cpus() -> usize {
    std::thread::available_parallelism().map_or(4, std::num::NonZero::get)
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();

    banner::print_banner(cli.verbose);

    let result: anyhow::Result<()> = match cli.command {
        Command::Version => {
            println!("picroom {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        Command::Api { bind } => api_cmd::run(cli.config, bind).await,
        Command::Worker { concurrency } => worker_cmd::run(cli.config, concurrency).await,
        Command::Admin(AdminCmd::Migrate { action }) => match action {
            MigrateAction::Run => {
                let path = cli.config.as_deref();
                match open_db(path).await {
                    Ok(db) => match picroom_admin::migrate_run(&db).await {
                        Ok(()) => {
                            println!("migrations applied");
                            Ok(())
                        }
                        Err(e) => Err(anyhow::anyhow!("{e}")),
                    },
                    Err(e) => Err(anyhow::anyhow!("open db: {e}")),
                }
            }
            MigrateAction::Revert | MigrateAction::Status => Err(anyhow::anyhow!(
                "migrate {action:?} not implemented in skeleton"
            )),
        },
        Command::Admin(AdminCmd::User(cmd)) => {
            run_user_cmd(cmd).await.map_err(|e| anyhow::anyhow!("{e}"))
        }
        Command::Admin(AdminCmd::Team(cmd)) => {
            run_team_cmd(cmd).await.map_err(|e| anyhow::anyhow!("{e}"))
        }
        Command::Admin(AdminCmd::Audit { follow, actor }) => {
            match picroom_admin::audit_tail(follow, actor).await {
                Ok(events) => {
                    for ev in events {
                        println!("{} {}", ev.timestamp, ev.action.as_str());
                    }
                    Ok(())
                }
                Err(e) => Err(anyhow::anyhow!("{e}")),
            }
        }
        Command::Admin(AdminCmd::Config(cmd)) => match cmd {
            ConfigCmd::Print => picroom_admin::config_print().map_err(|e| anyhow::anyhow!("{e}")),
            ConfigCmd::Validate => {
                picroom_admin::config_validate().map_err(|e| anyhow::anyhow!("{e}"))
            }
        },
        Command::Admin(AdminCmd::StorageTest { policy: _ }) => {
            // Placeholder: real impl reads storage config and constructs driver.
            Err(anyhow::anyhow!("storage test not implemented in skeleton"))
        }
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            tracing::error!("{e:?}");
            ExitCode::FAILURE
        }
    }
}

async fn open_db(config_path: Option<&std::path::Path>) -> anyhow::Result<picroom_infra::Database> {
    let cfg =
        picroom_infra::load_config_from(config_path).map_err(|e| anyhow::anyhow!("config: {e}"))?;
    picroom_infra::Database::connect(&cfg.database.url)
        .await
        .map_err(|e| anyhow::anyhow!("db: {e}"))
}

fn parse_role(s: &str) -> picroom_auth::Role {
    use picroom_auth::Role;
    match s {
        "admin" => Role::Admin,
        "manager" => Role::Manager,
        "uploader" => Role::Uploader,
        _ => Role::Viewer,
    }
}

async fn run_user_cmd(cmd: picroom_admin::UserCmd) -> anyhow::Result<()> {
    use picroom_admin::user::{
        user_create_sqlite, user_disable_sqlite, user_list_sqlite, user_set_role_sqlite,
    };
    let url = std::env::var("PICROOM_DATABASE__URL")
        .map_err(|_| anyhow::anyhow!("PICROOM_DATABASE__URL must be set"))?;
    let pool = picroom_admin::user::open_pool(&url).await?;
    let pool = match pool {
        picroom_admin::user::AnyPool::Sqlite(p) => p,
        picroom_admin::user::AnyPool::Pg(_) => {
            anyhow::bail!("Pg admin commands are not yet wired in the binary")
        }
    };
    match cmd {
        picroom_admin::UserCmd::Create {
            email,
            name,
            password,
            role,
        } => {
            let _id = user_create_sqlite(&pool, email, name, password, parse_role(&role))
                .await
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            println!("user created");
            Ok(())
        }
        picroom_admin::UserCmd::List => {
            for (_id, email, role) in user_list_sqlite(&pool)
                .await
                .map_err(|e| anyhow::anyhow!("{e}"))?
            {
                println!("{email}\t{role:?}");
            }
            Ok(())
        }
        picroom_admin::UserCmd::SetRole { user_id, role } => {
            user_set_role_sqlite(&pool, user_id, parse_role(&role))
                .await
                .map_err(|e| anyhow::anyhow!("{e}"))
        }
        picroom_admin::UserCmd::Disable { user_id } => user_disable_sqlite(&pool, user_id)
            .await
            .map_err(|e| anyhow::anyhow!("{e}")),
    }
}

async fn run_team_cmd(cmd: picroom_admin::TeamCmd) -> anyhow::Result<()> {
    use picroom_admin::team::{team_add_member_sqlite, team_create_sqlite, team_list_sqlite};
    let url = std::env::var("PICROOM_DATABASE__URL")
        .map_err(|_| anyhow::anyhow!("PICROOM_DATABASE__URL must be set"))?;
    let pool = picroom_admin::user::open_pool(&url).await?;
    let pool = match pool {
        picroom_admin::user::AnyPool::Sqlite(p) => p,
        picroom_admin::user::AnyPool::Pg(_) => {
            anyhow::bail!("Pg admin commands are not yet wired in the binary")
        }
    };
    match cmd {
        picroom_admin::TeamCmd::Create { name, slug } => {
            let _id = team_create_sqlite(&pool, name, slug)
                .await
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            println!("team created");
            Ok(())
        }
        picroom_admin::TeamCmd::List => {
            for t in team_list_sqlite(&pool)
                .await
                .map_err(|e| anyhow::anyhow!("{e}"))?
            {
                println!("{}\t{}", t.0, t.1);
            }
            Ok(())
        }
        picroom_admin::TeamCmd::AddMember {
            team_id,
            user_id,
            role,
        } => team_add_member_sqlite(&pool, team_id, user_id, parse_role(&role))
            .await
            .map_err(|e| anyhow::anyhow!("{e}")),
    }
}
