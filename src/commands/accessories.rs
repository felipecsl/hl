use anyhow::Result;
use clap::{Args, Subcommand};
use hl::config::{app_dir, load_config, systemd_dir};
use hl::discovery::{discover_accessories, discover_processes};
use hl::docker::{wait_for_postgres_ready, wait_for_redis_ready};
use hl::log::*;
use hl::systemd::{enable_accessories, reload_systemd_daemon, restart_app_target, write_unit};
use rand::Rng;
use std::os::unix::fs::PermissionsExt;
use tokio::fs;

#[derive(Args)]
pub struct AccessoriesArgs {
    #[command(subcommand)]
    pub command: AccessoriesCommand,
}

#[derive(Subcommand)]
pub enum AccessoriesCommand {
    /// Add an accessory to an app
    Add(AddArgs),
}

#[derive(Args)]
pub struct AddArgs {
    /// Application name
    #[arg(long)]
    pub app: String,

    /// Accessory type (e.g., postgres, redis)
    pub accessory: String,

    /// Version (default: 17 for postgres, 7 for redis)
    #[arg(long)]
    pub version: Option<String>,

    /// Postgres username (defaults to app name)
    #[arg(long)]
    pub user: Option<String>,

    /// Postgres database name (defaults to app name)
    #[arg(long)]
    pub database: Option<String>,

    /// Postgres password (generates random if not provided)
    #[arg(long)]
    pub password: Option<String>,
}

pub async fn execute(opts: AccessoriesArgs) -> Result<()> {
    match opts.command {
        AccessoriesCommand::Add(args) => execute_add(args).await,
    }
}

async fn execute_add(opts: AddArgs) -> Result<()> {
    match opts.accessory.as_str() {
        "postgres" => add_postgres(opts).await,
        "redis" => add_redis(opts).await,
        _ => {
            anyhow::bail!("unsupported accessory type: {}", opts.accessory);
        }
    }
}

/// Verify that the app directory exists
fn ensure_app_dir_exists(app: &str) -> Result<std::path::PathBuf> {
    let dir = app_dir(app);
    if !dir.exists() {
        anyhow::bail!(
            "app directory does not exist: {}. Run 'hl init' first.",
            dir.display()
        );
    }
    Ok(dir)
}

async fn add_postgres(opts: AddArgs) -> Result<()> {
    let dir = ensure_app_dir_exists(&opts.app)?;

    // Set defaults
    let version = opts.version.unwrap_or_else(|| "17".to_string());
    let user = opts.user.unwrap_or_else(|| opts.app.clone());
    let database = opts.database.unwrap_or_else(|| opts.app.clone());
    let password = opts.password.unwrap_or_else(generate_password);

    // Load config to get the network name
    let config = load_config(&opts.app).await?;
    let network = config.network;

    let compose_postgres = format!(
        r#"services:
  {}:
    depends_on:
      pg:
        condition: service_healthy

  pg:
    image: postgres:{}
    container_name: {}_pg
    restart: unless-stopped
    environment:
      POSTGRES_USER: ${{POSTGRES_USER}}
      POSTGRES_PASSWORD: ${{POSTGRES_PASSWORD}}
      POSTGRES_DB: ${{POSTGRES_DB}}
    volumes:
      - ./pgdata:/var/lib/postgresql/data
    networks: [{}]
    expose: ["5432"]
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U $$POSTGRES_USER -d $$POSTGRES_DB || exit 1"]
      interval: 5s
      timeout: 3s
      retries: 10

networks:
  {}:
    external: true
    name: {}
"#,
        opts.app, version, opts.app, network, network, network
    );

    let postgres_compose_path = dir.join("compose.postgres.yml");
    fs::write(&postgres_compose_path, compose_postgres).await?;

    ok(&format!("created {}", postgres_compose_path.display()));

    // Update .env file
    let env_path = dir.join(".env");
    let mut env_content = if env_path.exists() {
        fs::read_to_string(&env_path).await?
    } else {
        String::new()
    };

    // Check if postgres variables already exist
    let has_postgres_user = env_content.contains("POSTGRES_USER=");
    let has_postgres_password = env_content.contains("POSTGRES_PASSWORD=");
    let has_postgres_db = env_content.contains("POSTGRES_DB=");
    let has_database_url = env_content.contains("DATABASE_URL=");

    // Build the DATABASE_URL
    let database_url = format!("postgres://{}:{}@pg:5432/{}", user, password, database);

    // Append missing variables
    let mut additions = Vec::new();

    if !has_postgres_user {
        additions.push(format!("POSTGRES_USER={}", user));
    }
    if !has_postgres_password {
        additions.push(format!("POSTGRES_PASSWORD={}", password));
    }
    if !has_postgres_db {
        additions.push(format!("POSTGRES_DB={}", database));
    }
    if !has_database_url {
        additions.push(format!("DATABASE_URL={}", database_url));
    }

    if !additions.is_empty() {
        // Ensure the file ends with a newline before appending
        if !env_content.is_empty() && !env_content.ends_with('\n') {
            env_content.push('\n');
        }

        env_content.push_str(&additions.join("\n"));
        env_content.push('\n');

        // Write the updated content
        fs::write(&env_path, &env_content).await?;

        // Set permissions to 600
        let mut perms = fs::metadata(&env_path).await?.permissions();
        perms.set_mode(0o600);
        fs::set_permissions(&env_path, perms).await?;

        ok(&format!(
            "updated {} with postgres credentials (chmod 600)",
            env_path.display()
        ));
    } else {
        log("all postgres environment variables already exist in .env");
    }

    // Regenerate the systemd unit to include the new compose.postgres.yml file
    let systemd_dir = systemd_dir();
    let processes = discover_processes(&systemd_dir, &opts.app)?;
    let accessories = discover_accessories(&systemd_dir, &dir, &opts.app, &processes)?;
    write_unit(&opts.app, &processes, &accessories).await?;
    ok("regenerated systemd unit file to include postgres compose file");
    reload_systemd_daemon().await?;
    enable_accessories(&opts.app).await?;
    log("waiting for postgres to be ready...");
    wait_for_postgres_ready(&opts.app).await?;
    ok("postgres is ready");
    restart_app_target(&opts.app).await?;

    Ok(())
}

/// Generate a random strong password (alphanumeric only to avoid URI encoding issues)
fn generate_password() -> String {
    const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    const PASSWORD_LEN: usize = 32;
    let mut rng = rand::rng();

    (0..PASSWORD_LEN)
        .map(|_| {
            let idx = rng.random_range(0..CHARSET.len());
            CHARSET[idx] as char
        })
        .collect()
}

async fn add_redis(opts: AddArgs) -> Result<()> {
    let dir = ensure_app_dir_exists(&opts.app)?;

    // Set default version
    let version = opts.version.unwrap_or_else(|| "7".to_string());

    // Load config to get the network name
    let config = load_config(&opts.app).await?;
    let network = config.network;

    let compose_redis = format!(
        r#"services:
  {}:
    depends_on:
      redis:
        condition: service_healthy

  redis:
    image: redis:{}
    container_name: {}_redis
    restart: unless-stopped
    volumes:
      - ./redisdata:/data
    networks: [{}]
    expose: ["6379"]
    healthcheck:
      test: ["CMD", "redis-cli", "ping"]
      interval: 5s
      timeout: 3s
      retries: 10

networks:
  {}:
    external: true
    name: {}
"#,
        opts.app, version, opts.app, network, network, network
    );

    let redis_compose_path = dir.join("compose.redis.yml");
    fs::write(&redis_compose_path, compose_redis).await?;

    ok(&format!("created {}", redis_compose_path.display()));

    // Update .env file
    let env_path = dir.join(".env");
    let mut env_content = if env_path.exists() {
        fs::read_to_string(&env_path).await?
    } else {
        String::new()
    };

    // Check if Redis URL already exists
    let has_redis_url = env_content.contains("REDIS_URL=");

    if !has_redis_url {
        // Ensure the file ends with a newline before appending
        if !env_content.is_empty() && !env_content.ends_with('\n') {
            env_content.push('\n');
        }

        let redis_url = format!("REDIS_URL=redis://{}_redis:6379/0\n", opts.app);
        env_content.push_str(&redis_url);

        // Write the updated content
        fs::write(&env_path, &env_content).await?;

        // Set permissions to 600
        let mut perms = fs::metadata(&env_path).await?.permissions();
        perms.set_mode(0o600);
        fs::set_permissions(&env_path, perms).await?;

        ok(&format!(
            "updated {} with REDIS_URL (chmod 600)",
            env_path.display()
        ));
    } else {
        log("REDIS_URL already exists in .env");
    }

    let systemd_dir = systemd_dir();
    let processes = discover_processes(&systemd_dir, &opts.app)?;
    let accessories = discover_accessories(&systemd_dir, &dir, &opts.app, &processes)?;
    write_unit(&opts.app, &processes, &accessories).await?;
    ok("regenerated systemd unit file to include redis compose file");
    reload_systemd_daemon().await?;
    enable_accessories(&opts.app).await?;
    log("waiting for redis to be ready...");
    wait_for_redis_ready(&opts.app).await?;
    ok("redis is ready");
    restart_app_target(&opts.app).await?;

    Ok(())
}
