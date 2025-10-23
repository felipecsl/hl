use anyhow::Result;
use clap::Args;
use hl::{
  config::{app_dir, hl_git_root, load_config, systemd_dir},
  discovery::discover_accessories,
  docker::*,
  env::load_build_env_contents,
  git::export_commit,
  health::wait_for_healthy,
  log::*,
  procfile::parse_procfile,
  systemd::{enable_accessories, reload_systemd_daemon, start_accessories, write_unit},
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

  // Check for Procfile and parse if present
  let procfile_path = worktree.join("Procfile");
  let processes = if procfile_path.exists() {
    debug("found Procfile, parsing processes");
    let procs = parse_procfile(&procfile_path).await?;
    debug(&format!("parsed {} processes from Procfile", procs.len()));
    for (name, cmd) in &procs {
      debug(&format!("  {}: {}", name, cmd));
    }
    Some(procs)
  } else {
    debug("no Procfile found, using default configuration");
    None
  };

  let cfg = load_config(&opts.app).await?;

  // Generate process-specific compose files
  log("generating process compose files");
  let app_directory = app_dir(&cfg.app);
  write_process_compose_files(&app_directory, processes.as_ref(), &cfg.app, &cfg.resolver).await?;

  let systemd_dir = systemd_dir();
  let process_names = processes
    .map(|p| p.keys().cloned().collect::<Vec<String>>())
    .unwrap_or_else(|| vec!["web".to_string()]);
  let accessories = discover_accessories(&systemd_dir, &app_directory, &opts.app, &process_names)?;
  write_unit(&opts.app, &process_names, &accessories).await?;

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

  // load build-time secrets from .env.build
  let secrets_map = load_build_env_contents(&opts.app)?;
  let secrets = secrets_map
    .into_iter()
    .map(|(k, v)| BuildSecret::from_kv(&k, &v))
    .collect::<Vec<BuildSecret>>();

  build_and_push(BuildPushOptions {
    context: worktree.to_string_lossy().to_string(),
    dockerfile: Some(dockerfile.to_string_lossy().to_string()),
    tags: vec![tags.sha.clone(), tags.branch_sha, tags.latest.clone()],
    platforms: Some(cfg.platforms.clone()),
    secrets,
  })
  .await?;

  wait_for_accessories(&cfg.app, &accessories).await?;

  log("running migrations");
  run_migrations(&cfg, &tags.sha).await?;

  log("retagging latest");
  retag_latest(&cfg.image, &tags.sha).await?;

  log("enabling systemd service");
  reload_systemd_daemon().await?;
  enable_accessories(&cfg.app).await?;

  log("restarting services");
  restart_compose(&cfg, &process_names, &accessories).await?;

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

async fn wait_for_accessories(app: &str, accessories: &[String]) -> Result<()> {
  if !accessories.is_empty() {
    // Ensure accessories are started and ready before running migrations
    log("enabling and starting accessories");
    start_accessories(app).await?;
    if accessories.contains(&"postgres".to_string()) {
      log("waiting for postgres to be ready...");
      wait_for_postgres_ready(app).await?;
      ok("postgres is ready");
    }
    if accessories.contains(&"redis".to_string()) {
      log("waiting for redis to be ready...");
      wait_for_redis_ready(app).await?;
      ok("redis is ready");
    }
  }
  Ok(())
}
