use anyhow::Result;
use clap::Args;
use hl::{git::infer_app_name, log::*};
use std::process::Stdio;
use tokio::process::Command;

#[derive(Args)]
pub struct LogsArgs {
  /// Follow log output (stream logs)
  #[arg(short, long)]
  pub follow: bool,

  /// Number of lines to show from the end of the logs
  #[arg(short = 'n', long)]
  pub tail: Option<String>,
}

pub async fn execute(args: LogsArgs) -> Result<()> {
  let app = infer_app_name().await?;

  let mut docker_args = vec!["logs".to_string()];

  if args.follow {
    docker_args.push("--follow".to_string());
  }

  if let Some(tail) = args.tail {
    docker_args.push("--tail".to_string());
    docker_args.push(tail);
  }

  docker_args.push(app.clone());

  debug(&format!("executing: docker {}", docker_args.join(" ")));

  let status = Command::new("docker")
    .args(&docker_args)
    .stdin(Stdio::inherit())
    .stdout(Stdio::inherit())
    .stderr(Stdio::inherit())
    .status()
    .await?;

  if !status.success() {
    anyhow::bail!("docker logs failed with status: {}", status);
  }

  Ok(())
}
