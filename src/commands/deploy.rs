use hl::{config::load_config, docker::*, health::wait_for_healthy, log::*};
use anyhow::Result;
use clap::Args;

#[derive(Args)]
pub struct DeployArgs {
    /// Application name
    #[arg(long)]
    pub app: String,

    /// Git commit SHA
    #[arg(long)]
    pub sha: String,

    /// Git branch name
    #[arg(long, default_value = "master")]
    pub branch: String,

    /// Build context directory
    #[arg(long, default_value = ".")]
    pub context: String,

    /// Dockerfile path
    #[arg(long, default_value = "Dockerfile")]
    pub dockerfile: String,
}

pub async fn execute(opts: DeployArgs) -> Result<()> {
    let cfg = load_config(&opts.app).await?;
    let tags = tag_for(&cfg, &opts.sha, &opts.branch);

    log(&format!(
        "building {} {} ({})",
        cfg.app,
        opts.branch,
        &opts.sha[..7.min(opts.sha.len())]
    ));
    build_and_push(BuildPushOptions {
        context: opts.context,
        dockerfile: Some(opts.dockerfile),
        tags: vec![tags.sha.clone(), tags.branch_sha, tags.latest.clone()],
        platforms: Some(cfg.platforms.clone()),
    })
    .await?;

    log("running migrations");
    run_migrations(&cfg, &tags.sha).await?;

    log("retagging latest");
    retag_latest(&cfg.image, &tags.sha).await?;

    log("restarting compose");
    restart_compose(&cfg).await?;

    log("waiting for health");
    wait_for_healthy(&cfg.health.url, &cfg.health.timeout, &cfg.health.interval).await?;

    ok("deploy complete");
    Ok(())
}
