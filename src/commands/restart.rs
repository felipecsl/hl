use anyhow::Result;
use clap::Args;
use hl::{git::infer_app_name, log::*, systemd::restart_app_target};

#[derive(Args)]
pub struct RestartArgs {}

pub async fn execute(_args: RestartArgs) -> Result<()> {
  let app = infer_app_name()?;
  log(&format!("restarting service for app: {}", app));
  restart_app_target(&app).await?;
  ok("restart complete");
  Ok(())
}
