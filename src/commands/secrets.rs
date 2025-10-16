use hl::config::{app_dir, env_file};
use anyhow::{Context, Result};
use clap::{Args, Subcommand};
use std::collections::HashMap;
use std::path::Path;
use tokio::fs;

#[derive(Args)]
pub struct SecretsArgs {
    #[command(subcommand)]
    pub command: SecretsCommands,
}

#[derive(Subcommand)]
pub enum SecretsCommands {
    /// Set environment variable secrets
    Set {
        /// Application name
        app: String,
        /// KEY=VALUE pairs
        pairs: Vec<String>,
    },
    /// List environment variable keys (values masked)
    Ls {
        /// Application name
        app: String,
    },
}

pub async fn execute(args: SecretsArgs) -> Result<()> {
    match args.command {
        SecretsCommands::Set { app, pairs } => set_secrets(&app, pairs).await,
        SecretsCommands::Ls { app } => list_secrets(&app).await,
    }
}

async fn set_secrets(app: &str, pairs: Vec<String>) -> Result<()> {
    let file_path = env_file(app);
    let dir = app_dir(app);
    fs::create_dir_all(&dir).await?;

    // Create file if it doesn't exist
    if !Path::new(&file_path).exists() {
        fs::write(&file_path, "").await?;
    }

    // Read existing content
    let text = fs::read_to_string(&file_path).await?;
    let mut map: HashMap<String, String> = HashMap::new();

    // Parse existing env vars
    for line in text.lines() {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(pos) = line.find('=') {
            if pos > 0 {
                map.insert(line[..pos].to_string(), line[pos + 1..].to_string());
            }
        }
    }

    // Update with new pairs
    for pair in pairs {
        let pos = pair
            .find('=')
            .context(format!("bad pair: {}", pair))?;
        if pos < 1 {
            anyhow::bail!("bad pair: {}", pair);
        }
        map.insert(pair[..pos].to_string(), pair[pos + 1..].to_string());
    }

    // Write back
    let mut entries: Vec<_> = map.iter().collect();
    entries.sort_by_key(|(k, _)| *k);
    let output: String = entries
        .iter()
        .map(|(k, v)| format!("{}={}", k, v))
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";

    fs::write(&file_path, output).await?;
    // Set restrictive permissions (owner read/write only)
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let permissions = std::fs::Permissions::from_mode(0o600);
        std::fs::set_permissions(&file_path, permissions)?;
    }

    println!("updated {}", file_path.display());
    Ok(())
}

async fn list_secrets(app: &str) -> Result<()> {
    let file_path = env_file(app);
    let text = fs::read_to_string(&file_path).await.unwrap_or_default();

    for line in text.lines() {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(pos) = line.find('=') {
            if pos > 0 {
                println!("{}=***", &line[..pos]);
            }
        }
    }

    Ok(())
}
