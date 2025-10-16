use crate::config::parse_duration;
use anyhow::Result;
use std::time::{Duration, Instant};
use tokio::time::sleep;

pub async fn wait_for_healthy(url: &str, timeout: &str, interval: &str) -> Result<()> {
    let timeout_ms = parse_duration(timeout)?;
    let interval_ms = parse_duration(interval)?;

    let timeout_duration = Duration::from_millis(timeout_ms);
    let interval_duration = Duration::from_millis(interval_ms);

    let start = Instant::now();
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()?;

    while start.elapsed() < timeout_duration {
        if ping(&client, url).await {
            return Ok(());
        }
        sleep(interval_duration).await;
    }

    anyhow::bail!("health check timed out: {}", url)
}

async fn ping(client: &reqwest::Client, url: &str) -> bool {
    match client.get(url).send().await {
        Ok(resp) => resp.status().is_success() || resp.status().is_redirection(),
        Err(_) => false,
    }
}
