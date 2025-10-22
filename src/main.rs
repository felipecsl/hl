mod commands;

use anyhow::Result;
use clap::{Parser, Subcommand};
use hl::log::set_verbose;

#[derive(Parser)]
#[command(name = "hl")]
#[command(about = "Homelab deploy toolbox", long_about = None)]
struct Cli {
  /// Enable verbose/debug logging
  #[arg(short, long, global = true)]
  verbose: bool,

  #[command(subcommand)]
  command: Commands,
}

#[derive(Subcommand)]
enum Commands {
  /// Manage accessories (postgres, redis, etc.)
  Accessory(commands::accessory::AccessoriesArgs),
  /// Build->push->migrate->restart->health (invoke from post-receive)
  Deploy(commands::deploy::DeployArgs),
  /// Initializes a new app with its configuration files
  Init(commands::init::InitArgs),
  /// Stream logs from a service
  Logs(commands::logs::LogsArgs),
  /// Restart a service using systemctl
  Restart(commands::restart::RestartArgs),
  /// Retag :latest to a previous sha and restart (health-gated)
  Rollback(commands::rollback::RollbackArgs),
  /// Manage .env environment variables
  Env(commands::env::EnvArgs),
  /// Teardown an app (stop services, remove files, directories and git repo)
  Teardown(commands::teardown::TeardownArgs),
}

#[tokio::main]
async fn main() -> Result<()> {
  let cli = Cli::parse();

  // Set verbose mode
  set_verbose(cli.verbose);

  match cli.command {
    Commands::Accessory(args) => commands::accessory::execute(args).await?,
    Commands::Deploy(args) => commands::deploy::execute(args).await?,
    Commands::Init(args) => commands::init::execute(args).await?,
    Commands::Logs(args) => commands::logs::execute(args).await?,
    Commands::Restart(args) => commands::restart::execute(args).await?,
    Commands::Rollback(args) => commands::rollback::execute(args).await?,
    Commands::Env(args) => commands::env::execute(args).await?,
    Commands::Teardown(args) => commands::teardown::execute(args).await?,
  }

  Ok(())
}
