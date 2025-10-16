use hl::{config::load_config, docker::*, health::wait_for_healthy, log::*};
use anyhow::Result;
use clap::Args;

#[derive(Args)]
pub struct RollbackArgs {
    /// Application name
    pub app: String,

    /// Commit SHA or image short tag
    pub sha: String,
}

pub async fn execute(args: RollbackArgs) -> Result<()> {
    let cfg = load_config(&args.app).await?;
    let short_sha = &args.sha[..7.min(args.sha.len())];
    let from = format!("{}:{}", cfg.image, short_sha);

    log(&format!("retagging {} -> {}:latest", from, cfg.image));
    retag_latest(&cfg.image, &from).await?;

    log("restarting compose");
    restart_compose(&cfg).await?;

    log("waiting for health");
    wait_for_healthy(&cfg.health.url, &cfg.health.timeout, &cfg.health.interval).await?;

    ok("rollback complete");
    Ok(())
}
