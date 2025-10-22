use anyhow::{Context, Result};
use clap::{Args, Subcommand};
use hl::{
  config::{app_dir, build_env_file, env_file},
  env::load_env_file_contents,
};
use std::path::Path;
use tokio::fs;

#[derive(Args)]
pub struct EnvArgs {
  #[command(subcommand)]
  pub command: EnvCommands,
}

#[derive(Subcommand)]
pub enum EnvCommands {
  /// Set environment variables
  Set {
    /// Application name
    app: String,
    /// KEY=VALUE pairs
    pairs: Vec<String>,
    /// Store as build-time secrets
    #[arg(long)]
    build: bool,
  },
  /// List environment variable keys (values masked)
  Ls {
    /// Application name
    app: String,
    /// List build-time secrets
    #[arg(long)]
    build: bool,
  },
}

pub async fn execute(args: EnvArgs) -> Result<()> {
  match args.command {
    EnvCommands::Set { app, pairs, build } => set_env(&app, pairs, build).await,
    EnvCommands::Ls { app, build } => list_env(&app, build).await,
  }
}

async fn set_env(app: &str, pairs: Vec<String>, build: bool) -> Result<()> {
  let file_path = if build {
    build_env_file(app)
  } else {
    env_file(app)
  };
  let dir = app_dir(app);
  fs::create_dir_all(&dir).await?;

  // Create file if it doesn't exist
  if !Path::new(&file_path).exists() {
    fs::write(&file_path, "").await?;
  }

  let mut map = load_env_file_contents(&file_path)?;

  // Update with new pairs
  for pair in pairs {
    let pos = pair.find('=').context(format!("bad pair: {}", pair))?;
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

async fn list_env(app: &str, build: bool) -> Result<()> {
  let file_path = if build {
    build_env_file(app)
  } else {
    env_file(app)
  };
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

#[cfg(test)]
mod tests {
  use super::*;
  use serial_test::serial;
  use tempfile::TempDir;

  #[tokio::test]
  #[serial]
  async fn test_set_and_list_env_runtime() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let app_name = "testapp";

    // Override hl_root to use temp directory
    std::env::set_var("HL_ROOT_OVERRIDE", temp_dir.path().to_str().unwrap());

    let pairs = vec![
      "DATABASE_URL=postgres://localhost/db".to_string(),
      "API_KEY=secret123".to_string(),
    ];

    set_env(app_name, pairs, false).await?;

    // Verify file was created and contains correct content
    let file_path = temp_dir.path().join(app_name).join(".env");
    assert!(file_path.exists());
    let content = fs::read_to_string(&file_path).await?;
    assert!(content.contains("DATABASE_URL=postgres://localhost/db"));
    assert!(content.contains("API_KEY=secret123"));

    // Test that keys are sorted alphabetically
    let lines: Vec<&str> = content.lines().collect();
    assert_eq!(lines[0], "API_KEY=secret123");
    assert_eq!(lines[1], "DATABASE_URL=postgres://localhost/db");

    // Clean up
    std::env::remove_var("HL_ROOT_OVERRIDE");

    Ok(())
  }

  #[tokio::test]
  #[serial]
  async fn test_set_and_list_env_build() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let app_name = "testapp";

    // Override hl_root to use temp directory
    std::env::set_var("HL_ROOT_OVERRIDE", temp_dir.path().to_str().unwrap());

    let pairs = vec![
      "NPM_TOKEN=npm_secret".to_string(),
      "RAILS_MASTER_KEY=rails_key".to_string(),
    ];

    set_env(app_name, pairs, true).await?;

    let file_path = temp_dir.path().join(app_name).join(".env.build");
    assert!(file_path.exists());
    let content = fs::read_to_string(&file_path).await?;
    assert!(content.contains("NPM_TOKEN=npm_secret"));
    assert!(content.contains("RAILS_MASTER_KEY=rails_key"));

    // Clean up
    std::env::remove_var("HL_ROOT_OVERRIDE");

    Ok(())
  }

  #[tokio::test]
  #[serial]
  async fn test_set_env_updates_existing() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let app_name = "testapp";

    // Override hl_root to use temp directory
    std::env::set_var("HL_ROOT_OVERRIDE", temp_dir.path().to_str().unwrap());

    // Set initial variables
    let initial_pairs = vec!["KEY1=value1".to_string(), "KEY2=value2".to_string()];
    set_env(app_name, initial_pairs, false).await?;

    // Update KEY2 and add KEY3
    let update_pairs = vec!["KEY2=updated_value".to_string(), "KEY3=value3".to_string()];
    set_env(app_name, update_pairs, false).await?;

    // Verify updates
    let file_path = temp_dir.path().join(app_name).join(".env");
    let content = fs::read_to_string(&file_path).await?;
    assert!(content.contains("KEY1=value1"));
    assert!(content.contains("KEY2=updated_value"));
    assert!(content.contains("KEY3=value3"));
    assert!(!content.contains("KEY2=value2"));

    // Clean up
    std::env::remove_var("HL_ROOT_OVERRIDE");

    Ok(())
  }

  #[tokio::test]
  #[serial]
  async fn test_set_env_ignores_comments_and_empty_lines() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let app_name = "testapp";

    // Override hl_root to use temp directory
    std::env::set_var("HL_ROOT_OVERRIDE", temp_dir.path().to_str().unwrap());

    // Create file with comments and empty lines
    let file_path = temp_dir.path().join(app_name).join(".env");
    fs::create_dir_all(file_path.parent().unwrap()).await?;
    fs::write(
      &file_path,
      "# This is a comment\nKEY1=value1\n\n# Another comment\nKEY2=value2\n",
    )
    .await?;

    // Now update with set_env - should preserve existing values and ignore comments
    let pairs = vec!["KEY3=value3".to_string()];
    set_env(app_name, pairs, false).await?;

    // Read and verify - comments should be gone but all keys should be present
    let content = fs::read_to_string(&file_path).await?;
    assert!(content.contains("KEY1=value1"));
    assert!(content.contains("KEY2=value2"));
    assert!(content.contains("KEY3=value3"));
    // Comments won't be preserved since we rewrite the file

    // Clean up
    std::env::remove_var("HL_ROOT_OVERRIDE");

    Ok(())
  }

  #[tokio::test]
  #[serial]
  async fn test_set_env_file_permissions() -> Result<()> {
    #[cfg(unix)]
    {
      use std::os::unix::fs::PermissionsExt;

      let temp_dir = TempDir::new()?;
      let app_name = "testapp";

      // Override hl_root to use temp directory
      std::env::set_var("HL_ROOT_OVERRIDE", temp_dir.path().to_str().unwrap());

      // Call set_env which should set correct permissions
      let pairs = vec!["KEY=value".to_string()];
      set_env(app_name, pairs, false).await?;

      // Verify permissions
      let file_path = temp_dir.path().join(app_name).join(".env");
      let metadata = std::fs::metadata(&file_path)?;
      let mode = metadata.permissions().mode();
      assert_eq!(mode & 0o777, 0o600, "File should have 0600 permissions");

      // Clean up
      std::env::remove_var("HL_ROOT_OVERRIDE");
    }

    Ok(())
  }

  #[tokio::test]
  #[serial]
  async fn test_set_env_with_equals_in_value() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let app_name = "testapp";

    // Override hl_root to use temp directory
    std::env::set_var("HL_ROOT_OVERRIDE", temp_dir.path().to_str().unwrap());

    // Test that values can contain '=' characters
    let pair = "CONNECTION_STRING=server=localhost;user=admin;password=pass=123";
    let pairs = vec![pair.to_string()];

    // Call set_env
    set_env(app_name, pairs, false).await?;

    // Verify the value was stored correctly
    let file_path = temp_dir.path().join(app_name).join(".env");
    let content = fs::read_to_string(&file_path).await?;
    assert!(content.contains("CONNECTION_STRING=server=localhost;user=admin;password=pass=123"));

    // Clean up
    std::env::remove_var("HL_ROOT_OVERRIDE");

    Ok(())
  }
}
