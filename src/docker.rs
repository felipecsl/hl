use crate::config::{app_dir, env_file, HLConfig};
use crate::log::debug;
use crate::systemd::restart_service;
use anyhow::Result;
use std::path::Path;
use std::process::Stdio;
use tokio::process::Command;
use tokio::fs;

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

    restart_service(&cfg.app).await?;

    Ok(())
}

/// Build the docker run command arguments for migrations
fn build_migration_args(cfg: &HLConfig, image_tag: &str, env_path: &str) -> Vec<String> {
    let mut args = vec!["run".to_string(), "--rm".to_string()];

    // Add env file
    args.push("--env-file".to_string());
    args.push(env_path.to_string());

    // Add environment variables
    for (k, v) in &cfg.migrations.env {
        args.push("-e".to_string());
        args.push(format!("{}={}", k, v));
    }

    // Add network
    args.push("--network".to_string());
    args.push(cfg.network.clone());

    // Add image
    args.push(image_tag.to_string());

    // Add command
    for cmd_part in &cfg.migrations.command {
        args.push(cmd_part.clone());
    }

    args
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

    let args = build_migration_args(cfg, image_tag, &env_path_str);

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

/// Generate the base compose.yml file content for an application
pub async fn write_base_compose_file(
    dir: &Path,
    app: &str,
    image: &str,
    network: &str,
    resolver: &str,
) -> Result<()> {
    let compose = format!(
        r#"services:
  {}:
    image: {}:latest
    container_name: {}
    restart: unless-stopped
    env_file: [.env]
    networks: [{}]
    labels:
      - "traefik.enable=true"
      - "traefik.http.routers.{}.rule=Host(`${{{}}}`)"
      - "traefik.http.routers.{}.entrypoints=websecure"
      - "traefik.http.routers.{}.tls.certresolver={}"
      - "traefik.http.services.{}.loadbalancer.server.port=${{SERVICE_PORT}}"
networks:
  {}:
    external: true
    name: {}
"#,
        app,
        image,
        app,
        network,
        app,
        "DOMAIN",
        app,
        app,
        resolver,
        app,
        network,
        network
    );
    let compose_path = dir.join("compose.yml");
    fs::write(&compose_path, compose).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_write_base_compose_file() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let dir_path = temp_dir.path();
        let app = "testapp";
        let image = "registry.example.com/testapp";
        let network = "traefik_proxy";
        let resolver = "myresolver";
        write_base_compose_file(dir_path, app, image, network, resolver).await?;
        let compose_path = dir_path.join("compose.yml");
        assert!(compose_path.exists(), "compose.yml should be created");
        let content = fs::read_to_string(&compose_path).await?;
        let expected = r#"services:
  testapp:
    image: registry.example.com/testapp:latest
    container_name: testapp
    restart: unless-stopped
    env_file: [.env]
    networks: [traefik_proxy]
    labels:
      - "traefik.enable=true"
      - "traefik.http.routers.testapp.rule=Host(`${DOMAIN}`)"
      - "traefik.http.routers.testapp.entrypoints=websecure"
      - "traefik.http.routers.testapp.tls.certresolver=myresolver"
      - "traefik.http.services.testapp.loadbalancer.server.port=${SERVICE_PORT}"
networks:
  traefik_proxy:
    external: true
    name: traefik_proxy
"#;
        assert_eq!(content, expected, "Compose file content should match expected output");
        Ok(())
    }

    #[test]
    fn test_build_migration_args() {
        use std::collections::HashMap;

        // Create a test config with deterministic ordering by using a single env var
        let mut env_vars = HashMap::new();
        env_vars.insert("RAILS_ENV".to_string(), "production".to_string());

        let cfg = HLConfig {
            app: "testapp".to_string(),
            image: "registry.example.com/testapp".to_string(),
            domain: "testapp.example.com".to_string(),
            service_port: 3000,
            resolver: "myresolver".to_string(),
            network: "traefik_proxy".to_string(),
            platforms: "linux/amd64".to_string(),
            health: crate::config::HealthConfig {
                url: "http://testapp:3000/healthz".to_string(),
                interval: "2s".to_string(),
                timeout: "45s".to_string(),
            },
            migrations: crate::config::MigrationsConfig {
                command: vec!["bin/rails".to_string(), "db:migrate".to_string()],
                env: env_vars,
            },
            secrets: vec![],
        };

        let image_tag = "registry.example.com/testapp:abc1234";
        let env_path = "/home/user/prj/apps/testapp/.env";
        let args = build_migration_args(&cfg, image_tag, env_path);
        let result = args.join(" ");
        let expected = "run --rm --env-file /home/user/prj/apps/testapp/.env -e RAILS_ENV=production --network traefik_proxy registry.example.com/testapp:abc1234 bin/rails db:migrate";

        assert_eq!(result, expected, "Migration command should match expected output");
    }
}
