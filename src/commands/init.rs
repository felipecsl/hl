use hl::config::{hl_git_root, home_dir};
use hl::{config::app_dir, log::*, systemd::write_unit};
use anyhow::Result;
use clap::Args;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use tokio::fs;

#[derive(Args)]
pub struct InitArgs {
    /// Application name
    #[arg(long)]
    pub app: String,

    /// Docker image reference
    #[arg(long)]
    pub image: String,

    /// Domain name
    #[arg(long)]
    pub domain: String,

    /// Internal container port
    #[arg(long)]
    pub port: u16,

    /// Traefik network name. Defaults to "traefik_proxy"
    #[arg(long, default_value = "traefik_proxy")]
    pub network: String,

    /// ACME resolver name. Defaults to "myresolver"
    #[arg(long, default_value = "myresolver")]
    pub resolver: String,
}

pub async fn execute(opts: InitArgs) -> Result<()> {
    let dir = app_dir(&opts.app);
    fs::create_dir_all(&dir).await?;

    let env_path = dir.join(".env");
    if !Path::new(&env_path).exists() {
        let env_content = format!(
            "APP={}\nDOMAIN={}\nSERVICE_PORT={}\n",
            opts.app, opts.domain, opts.port
        );
        fs::write(&env_path, env_content).await?;
    }

    let compose = format!(
        r#"services:
  {}:
    image: {}:latest
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
        opts.app,
        opts.image,
        opts.network,
        opts.app,
        "DOMAIN",
        opts.app,
        opts.app,
        opts.resolver,
        opts.app,
        opts.network,
        opts.network
    );

    let compose_path = dir.join("compose.yml");
    fs::write(&compose_path, compose).await?;

    // TODO: hl currently makes a bunch of assumptions about the app being deployed:
    // - it's a Rails app and environment is production
    // - it uses RAILS_MASTER_KEY and SECRET_KEY_BASE secrets
    // - it runs migrations with "bin/rails db:migrate"
    // - it has a /healthz endpoint
    // We should make these configurable in the future.
    let hl_yml = format!(
        r#"app: {}
image: {}
domain: {}
servicePort: {}
resolver: {}
network: {}
platforms: linux/amd64
health:
  url: http://{}:{}/healthz
  interval: 2s
  timeout: 45s
migrations:
  command: ["bin/rails", "db:migrate"]
  env:
    RAILS_ENV: "production"
secrets:
  - RAILS_MASTER_KEY
  - SECRET_KEY_BASE
"#,
        opts.app,
        opts.image,
        opts.domain,
        opts.port,
        opts.resolver,
        opts.network,
        opts.app,
        opts.port
    );

    let hl_yml_path = dir.join("hl.yml");
    fs::write(&hl_yml_path, hl_yml).await?;

    let unit = write_unit(&opts.app).await?;

    log(&format!(
        "wrote {}, {} and {}",
        compose_path.display(),
        hl_yml_path.display(),
        env_path.display()
    ));
    ok(&format!("created {} (will be enabled on first deploy)", unit));

    // Create bare git repository
    let home = home_dir().to_string_lossy().to_string();
    let git_root = hl_git_root(opts.app.as_str());
    let git_dir = git_root.to_string_lossy().to_string();
    fs::create_dir_all(&git_root).await?;

    // Initialize bare git repository
    let status = tokio::process::Command::new("git")
        .arg("init")
        .arg("--bare")
        .arg(&git_dir)
        .status()
        .await?;

    if !status.success() {
        anyhow::bail!("failed to initialize git repository");
    }

    // Create post-receive hook
    let hooks_dir = git_root.join("hooks");
    let hook_path = hooks_dir.join("post-receive");

    let hook_content = format!(
        r#"#!/usr/bin/env bash
set -euo pipefail
while read -r oldrev newrev refname; do
  case "$refname" in refs/heads/*) branch="${{refname#refs/heads/}}";;
    *) continue;;
  esac
  {}/.hl/bin/hl deploy --app {} --sha "$newrev" --branch "$branch"
done
"#,
        home, opts.app
    );

    fs::write(&hook_path, hook_content).await?;

    // Make hook executable
    let mut perms = fs::metadata(&hook_path).await?.permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&hook_path, perms).await?;

    ok(&format!("created git repository at {}", &git_dir));

    // Get current user and hostname for git remote command
    let username = std::env::var("USER").unwrap_or_else(|_| "user".to_string());
    let hostname = std::process::Command::new("hostname")
        .output()
        .ok()
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "hostname".to_string());

    log(&format!(
        "To deploy from your local machine, add a git remote:\n  git remote add production ssh://{}@{}{}",
        username,
        hostname,
        git_dir
    ));

    Ok(())
}
