use anyhow::Result;
use clap::Args;
use hl::{
  config::{app_dir, load_config},
  discovery::{discover_accessories, discover_processes},
  docker::*,
  git::infer_app_name,
  health::wait_for_healthy,
  log::*,
};

#[derive(Args)]
pub struct RollbackArgs {
  /// Commit SHA or image short tag
  pub sha: String,
}

pub async fn execute(args: RollbackArgs) -> Result<()> {
  let app = infer_app_name()?;
  let cfg = load_config(&app).await?;
  let short_sha = &args.sha[..7.min(args.sha.len())];
  let from = format!("{}:{}", cfg.image, short_sha);

  log(&format!("retagging {} -> {}:latest", from, cfg.image));
  retag_latest(&cfg.image, &from).await?;

  log("restarting compose");
  let systemd_dir = hl::config::systemd_dir();
  let processes = discover_processes(&systemd_dir, &app)?;
  let accessories = discover_accessories(&systemd_dir, &app_dir(&app), &app, &processes)?;
  restart_compose(&cfg, &processes, &accessories).await?;

  log("waiting for healthchecks to pass");
  wait_for_healthy(&cfg).await?;

  ok("rollback complete");
  Ok(())
}
