use anyhow::Result;
use clap::{Args, Subcommand};
use hl::config::{app_dir, load_config};
use hl::log::*;
use hl::systemd::{restart_service, write_unit};
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

    /// Accessory type (e.g., postgres)
    pub accessory: String,

    /// Postgres version (default: 17)
    #[arg(long, default_value = "17")]
    pub version: String,

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
        _ => {
            anyhow::bail!("unsupported accessory type: {}", opts.accessory);
        }
    }
}

async fn add_postgres(opts: AddArgs) -> Result<()> {
    let dir = app_dir(&opts.app);

    // Verify app directory exists
    if !dir.exists() {
        anyhow::bail!(
            "app directory does not exist: {}. Run 'hl init' first.",
            dir.display()
        );
    }

    // Set defaults
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
        opts.app, opts.version, network, network, network
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
    write_unit(&opts.app).await?;
    ok("regenerated systemd unit file to include postgres compose file");

    restart_service(&opts.app).await?;

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
