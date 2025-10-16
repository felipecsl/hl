use hl::{config::app_dir, log::*, systemd::write_unit};
use anyhow::Result;
use clap::Args;
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

    /// Traefik network name
    #[arg(long, default_value = "traefik_proxy")]
    pub network: String,

    /// ACME resolver name
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

    let unit = write_unit(&opts.app).await?;

    log(&format!(
        "wrote {} and {}",
        compose_path.display(),
        env_path.display()
    ));
    ok(&format!("enabled {}", unit));

    Ok(())
}
