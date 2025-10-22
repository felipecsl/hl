use anyhow::Result;
use clap::Args;
use hl::{config::app_dir, log::*};
use std::process::Stdio;
use tokio::process::Command;

#[derive(Args)]
pub struct LogsArgs {
  /// Application name
  pub app: String,

  /// Follow log output (stream logs)
  #[arg(short, long)]
  pub follow: bool,

  /// Number of lines to show from the end of the logs
  #[arg(short = 'n', long)]
  pub tail: Option<String>,

  /// Show logs for specific service (default: all services)
  #[arg(short, long)]
  pub service: Option<String>,
}

pub async fn execute(args: LogsArgs) -> Result<()> {
  let dir = app_dir(&args.app);

  debug(&format!("logs: app_dir={}", dir.display()));

  if !dir.exists() {
    anyhow::bail!("App directory not found: {}", dir.display());
  }

  let compose_file = dir.join("compose.yml");
  if !compose_file.exists() {
    anyhow::bail!("compose.yml not found at: {}", compose_file.display());
  }

  // Build compose file list
  let mut compose_args = vec!["-f".to_string(), "compose.yml".to_string()];

  // Check for compose.postgres.yml
  let postgres_compose = dir.join("compose.postgres.yml");
  if postgres_compose.exists() {
    compose_args.push("-f".to_string());
    compose_args.push("compose.postgres.yml".to_string());
  }

  compose_args.push("logs".to_string());

  // Add follow flag
  if args.follow {
    compose_args.push("--follow".to_string());
  }

  // Add tail flag
  if let Some(tail) = args.tail {
    compose_args.push("--tail".to_string());
    compose_args.push(tail);
  }

  // Add specific service if specified
  if let Some(service) = args.service {
    compose_args.push(service);
  }

  debug(&format!(
    "executing docker compose command: docker compose {}",
    compose_args.join(" ")
  ));

  let status = Command::new("docker")
    .arg("compose")
    .args(&compose_args)
    .current_dir(&dir)
    .stdin(Stdio::inherit())
    .stdout(Stdio::inherit())
    .stderr(Stdio::inherit())
    .status()
    .await?;

  if !status.success() {
    anyhow::bail!("docker compose logs failed with status: {}", status);
  }

  Ok(())
}
