use crate::config::{app_dir, env_file, HLConfig};
use crate::log::debug;
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
    debug(&format!(
        "build_and_push: context={}, dockerfile={:?}",
        opts.context, opts.dockerfile
    ));

    // Verify context directory exists
    let context_path = std::path::Path::new(&opts.context);
    if !context_path.exists() {
        anyhow::bail!("Build context directory not found: {}", opts.context);
    }

    // Verify dockerfile exists if specified
    if let Some(ref dockerfile) = opts.dockerfile {
        let dockerfile_path = std::path::Path::new(dockerfile);
        if !dockerfile_path.exists() {
            anyhow::bail!("Dockerfile not found: {}", dockerfile);
        }
    }

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

    debug(&format!(
        "executing docker command: docker {}",
        args.join(" ")
    ));

    let status = Command::new("docker")
        .args(&args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .await?;

    if !status.success() {
        anyhow::bail!("docker build failed with status: {}", status);
    }

    debug("docker build completed successfully");

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

    debug(&format!("restart_compose: app_dir={}", dir.display()));

    if !dir.exists() {
        anyhow::bail!("App directory not found: {}", dir.display());
    }

    let compose_file = dir.join("compose.yml");
    if !compose_file.exists() {
        anyhow::bail!("compose.yml not found at: {}", compose_file.display());
    }

    debug("pulling latest images with docker compose");

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
        anyhow::bail!("docker compose pull failed with status: {}", status);
    }

    debug("restarting services with docker compose up -d");

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
        anyhow::bail!("docker compose up failed with status: {}", status);
    }

    debug("docker compose up completed successfully");

    Ok(())
}

pub async fn run_migrations(cfg: &HLConfig, image_tag: &str) -> Result<()> {
    let dir = app_dir(&cfg.app);
    let env_path = env_file(&cfg.app);
    let env_path_str = env_path.to_string_lossy().to_string();

    debug(&format!(
        "run_migrations: app_dir={}, env_file={}, image={}",
        dir.display(),
        env_path.display(),
        image_tag
    ));

    if !dir.exists() {
        anyhow::bail!("App directory not found: {}", dir.display());
    }

    if !env_path.exists() {
        debug(&format!(
            "Warning: .env file not found at: {}",
            env_path.display()
        ));
    }

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

    debug(&format!(
        "executing migrations with docker command: docker {}",
        args.join(" ")
    ));

    let status = Command::new("docker")
        .args(&args)
        .current_dir(&dir)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .await?;

    if !status.success() {
        anyhow::bail!("migrations failed with status: {}", status);
    }

    debug("migrations completed successfully");

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
