use anyhow::Result;
use clap::Args;
use hl::{log::*, systemd::restart_service};

#[derive(Args)]
pub struct RestartArgs {
    /// Application name
    #[arg(long)]
    pub app: String,
}

pub async fn execute(args: RestartArgs) -> Result<()> {
    log(&format!("restarting service for app: {}", args.app));
    restart_service(&args.app).await?;
    ok("restart complete");
    Ok(())
}
