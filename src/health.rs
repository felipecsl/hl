use crate::config::{parse_duration, HLConfig};
use anyhow::Result;
use std::process::Stdio;
use std::time::{Duration, Instant};
use tokio::{process::Command, time::sleep};

pub async fn wait_for_healthy(cfg: &HLConfig) -> Result<()> {
  let network = &cfg.network;
  let url = &cfg.health.url;
  let timeout = &cfg.health.timeout;
  let interval = &cfg.health.interval;
  let timeout_ms = parse_duration(timeout)?;
  let interval_ms = parse_duration(interval)?;
  let timeout_duration = Duration::from_millis(timeout_ms);
  let interval_duration = Duration::from_millis(interval_ms);
  let start = Instant::now();

  while start.elapsed() < timeout_duration {
    if curl_in_network(network, url).await {
      return Ok(());
    }
    sleep(interval_duration).await;
  }

  anyhow::bail!("health check timed out in docker network: {}", url)
}

async fn curl_in_network(network: &str, url: &str) -> bool {
  let status = Command::new("docker")
    .args([
      "run",
      "--rm",
      "--network",
      network,
      "curlimages/curl:8.16.0",
      "-fsS",
      "-m",
      "3",
      url,
    ])
    .stdin(Stdio::null())
    .stdout(Stdio::null())
    .stderr(Stdio::null())
    .status()
    .await;

  match status {
    Ok(status) => status.success(),
    Err(_) => false,
  }
}

pub async fn wait_for_healthy_http(url: &str, timeout: &str, interval: &str) -> Result<()> {
  let timeout_ms = parse_duration(timeout)?;
  let interval_ms = parse_duration(interval)?;
  let timeout_duration = Duration::from_millis(timeout_ms);
  let interval_duration = Duration::from_millis(interval_ms);
  let start = Instant::now();
  let client = reqwest::Client::builder()
    .timeout(Duration::from_secs(3))
    .build()?;

  while start.elapsed() < timeout_duration {
    if ping_http(&client, url).await {
      return Ok(());
    }
    sleep(interval_duration).await;
  }

  anyhow::bail!("health check timed out: {}", url)
}

async fn ping_http(client: &reqwest::Client, url: &str) -> bool {
  match client.get(url).send().await {
    Ok(resp) => resp.status().is_success() || resp.status().is_redirection(),
    Err(_) => false,
  }
}
