mod commands;

use clap::{Parser, Subcommand};
use anyhow::Result;
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
    /// Build->push->migrate->restart->health (invoke from post-receive)
    Deploy(commands::deploy::DeployArgs),
    /// Initializes a new app with its configuration files
    Init(commands::init::InitArgs),
    /// Retag :latest to a previous sha and restart (health-gated)
    Rollback(commands::rollback::RollbackArgs),
    /// Manage .env secrets
    Secrets(commands::secrets::SecretsArgs),
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Set verbose mode
    set_verbose(cli.verbose);

    match cli.command {
        Commands::Deploy(args) => commands::deploy::execute(args).await?,
        Commands::Init(args) => commands::init::execute(args).await?,
        Commands::Rollback(args) => commands::rollback::execute(args).await?,
        Commands::Secrets(args) => commands::secrets::execute(args).await?,
    }

    Ok(())
}
