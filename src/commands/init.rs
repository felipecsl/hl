use anyhow::Result;
use clap::Args;
use hl::config::{hl_git_root, home_dir};
use hl::docker::write_base_compose_file;
use hl::git::{init_bare_repo, repo_remote_uri};
use hl::{config::app_dir, log::*, systemd::write_unit};
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

    write_base_compose_file(&dir, &opts.app, &opts.image, &opts.network, &opts.resolver).await?;
    let compose_path = dir.join("compose.yml");

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
    ok(&format!(
        "created {} (will be enabled on first deploy)",
        unit
    ));

    // Create bare git repository
    let home = home_dir().to_string_lossy().to_string();
    let git_root = hl_git_root(opts.app.as_str());
    let git_dir = git_root.to_string_lossy().to_string();

    init_bare_repo(&git_root, &opts.app, &home).await?;

    ok(&format!("created git repository at {}", &git_dir));

    let git_uri = repo_remote_uri(&git_dir);
    log(&format!(
        "To deploy from your local machine, add a git remote:\n  git remote add production {}",
        git_uri
    ));

    Ok(())
}
