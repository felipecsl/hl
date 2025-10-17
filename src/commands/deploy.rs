use anyhow::Result;
use clap::Args;
use hl::{
    config::{hl_git_root, load_config},
    docker::*,
    git::export_commit,
    health::wait_for_healthy,
    log::*,
    systemd::enable_service,
};

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
}

pub async fn execute(opts: DeployArgs) -> Result<()> {
    // Export the commit to a temporary directory
    let repo_path = hl_git_root(&opts.app)
        .to_str()
        .expect("repo path is not valid UTF-8")
        .to_string();

    debug(&format!("repository path: {}", repo_path));

    let worktree = export_commit(&repo_path, &opts.sha).await?;

    debug(&format!("exported worktree to: {}", worktree.display()));

    let cfg = load_config(&opts.app).await?;
    let tags = tag_for(&cfg, &opts.sha, &opts.branch);

    log(&format!(
        "building {} {} ({})",
        cfg.app,
        opts.branch,
        &opts.sha[..7.min(opts.sha.len())]
    ));

    // Build using the exported worktree
    let dockerfile = worktree.join("Dockerfile");

    debug(&format!("dockerfile path: {}", dockerfile.display()));

    // Check if Dockerfile exists
    if !dockerfile.exists() {
        anyhow::bail!("Dockerfile not found at: {}", dockerfile.display());
    }

    debug(&format!("build context: {}", worktree.display()));

    build_and_push(BuildPushOptions {
        context: worktree.to_string_lossy().to_string(),
        dockerfile: Some(dockerfile.to_string_lossy().to_string()),
        tags: vec![tags.sha.clone(), tags.branch_sha, tags.latest.clone()],
        platforms: Some(cfg.platforms.clone()),
    })
    .await?;

    // TODO: This step hangs forever if the database container is not running
    // Might need to ensure service is running before applying migrations
    log("running migrations");
    run_migrations(&cfg, &tags.sha).await?;

    log("retagging latest");
    retag_latest(&cfg.image, &tags.sha).await?;

    log("enabling systemd service");
    enable_service(&cfg.app).await?;

    log("restarting compose");
    restart_compose(&cfg).await?;

    log("waiting for health");
    wait_for_healthy(
        &cfg.network,
        &cfg.health.url,
        &cfg.health.timeout,
        &cfg.health.interval,
    )
    .await?;

    // Clean up the temporary worktree
    if let Err(e) = tokio::fs::remove_dir_all(&worktree).await {
        eprintln!(
            "Warning: failed to cleanup worktree at {}: {}",
            worktree.display(),
            e
        );
    }

    ok("deploy complete");
    Ok(())
}
