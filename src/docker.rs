use crate::config::{app_dir, env_file, HLConfig};
use anyhow::Result;
use std::process::Stdio;
use tokio::process::Command;

pub struct BuildPushOptions {
    pub context: String,
    pub dockerfile: Option<String>,
    pub tags: Vec<String>,
    pub platforms: Option<String>,
}

pub async fn build_and_push(opts: BuildPushOptions) -> Result<()> {
    let mut args = vec!["buildx", "build", "--push"];

    if let Some(platforms) = &opts.platforms {
        args.push("--platform");
        args.push(platforms);
    }

    for tag in &opts.tags {
        args.push("-t");
        args.push(tag);
    }

    if let Some(dockerfile) = &opts.dockerfile {
        args.push("--file");
        args.push(dockerfile);
    }

    args.push(&opts.context);

    let status = Command::new("docker")
        .args(&args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .await?;

    if !status.success() {
        anyhow::bail!("docker build failed");
    }

    Ok(())
}

pub async fn retag_latest(image: &str, from_tag: &str) -> Result<()> {
    // Pull the source image
    let status = Command::new("docker")
        .args(["pull", from_tag])
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .await?;

    if !status.success() {
        anyhow::bail!("docker pull failed");
    }

    // Tag it as latest
    let latest = format!("{}:latest", image);
    let status = Command::new("docker")
        .args(["tag", from_tag, &latest])
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .await?;

    if !status.success() {
        anyhow::bail!("docker tag failed");
    }

    // Push latest
    let status = Command::new("docker")
        .args(["push", &latest])
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .await?;

    if !status.success() {
        anyhow::bail!("docker push failed");
    }

    Ok(())
}

pub async fn restart_compose(cfg: &HLConfig) -> Result<()> {
    let dir = app_dir(&cfg.app);

    // Pull latest images
    let status = Command::new("docker")
        .args(["compose", "-f", "compose.yml", "pull"])
        .current_dir(&dir)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .await?;

    if !status.success() {
        anyhow::bail!("docker compose pull failed");
    }

    // Restart services
    let status = Command::new("docker")
        .args(["compose", "-f", "compose.yml", "up", "-d"])
        .current_dir(&dir)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .await?;

    if !status.success() {
        anyhow::bail!("docker compose up failed");
    }

    Ok(())
}

pub async fn run_migrations(cfg: &HLConfig, image_tag: &str) -> Result<()> {
    let dir = app_dir(&cfg.app);
    let env_path = env_file(&cfg.app);
    let env_path_str = env_path.to_string_lossy().to_string();

    let mut args = vec!["run", "--rm"];

    // Add env file
    args.push("--env-file");
    args.push(&env_path_str);

    // Add environment variables
    let mut env_pairs = Vec::new();
    for (k, v) in &cfg.migrations.env {
        env_pairs.push(format!("{}={}", k, v));
    }
    for pair in &env_pairs {
        args.push("-e");
        args.push(pair);
    }

    // Add network
    args.push("--network");
    args.push(&cfg.network);

    // Add image
    args.push(image_tag);

    // Add command
    for cmd_part in &cfg.migrations.command {
        args.push(cmd_part);
    }

    let status = Command::new("docker")
        .args(&args)
        .current_dir(&dir)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .await?;

    if !status.success() {
        anyhow::bail!("migrations failed");
    }

    Ok(())
}

pub struct ImageTags {
    pub sha: String,
    pub branch_sha: String,
    pub latest: String,
}

pub fn tag_for(cfg: &HLConfig, sha: &str, branch: &str) -> ImageTags {
    let short = &sha[..7.min(sha.len())];
    ImageTags {
        sha: format!("{}:{}", cfg.image, short),
        branch_sha: format!("{}:{}-{}", cfg.image, branch, short),
        latest: format!("{}:latest", cfg.image),
    }
}
