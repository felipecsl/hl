use anyhow::{Context, Result};
use std::collections::HashMap;
use tokio::fs;

use crate::{
  config::build_env_file,
  log::{debug, log},
};

/// Read environment variable key-value pairs from a .env (or .env.build) file
/// # Arguments
/// * `file_path` - Path to the .env file
/// # Returns
/// HashMap mapping environment variable names to their values
pub fn load_env_file_contents(path: &std::path::Path) -> Result<HashMap<String, String>> {
  let mut map = HashMap::new();
  for item in dotenvy::from_path_iter(path)? {
    let (k, v) = item?;
    debug(&format!("Loaded env var: {}=****", k));
    map.insert(k, v);
  }
  Ok(map)
}

/// Write environment variable key-value pairs to a .env file
/// # Arguments
/// * `path` - Path to the .env file
/// * `content` - HashMap mapping environment variable names to their values
/// # Returns
/// Empty result on success
pub async fn write_env_file_contents(
  path: &std::path::Path,
  content: &HashMap<String, String>,
) -> Result<()> {
  // Convert HashMap to sorted string content
  let mut sorted_keys: Vec<_> = content.keys().collect();
  sorted_keys.sort();

  let mut file_content = String::new();
  for key in sorted_keys {
    if let Some(value) = content.get(key) {
      file_content.push_str(&format!("{}={}\n", key, value));
    }
  }

  fs::write(&path, &file_content).await?;
  Ok(())
}

/// Load build environment variables for the given app
/// # Arguments
/// * `app` - Application name
/// # Returns
/// HashMap mapping environment variable names to their values
/// # Notes
/// If the build environment file does not exist, returns an empty map
pub fn load_build_env_contents(app: &str) -> Result<HashMap<String, String>> {
  log("loading build environment secrets...");
  let build_env_path = build_env_file(app);
  if build_env_file(app).exists() {
    // Optional: warn if perms are too loose
    #[cfg(unix)]
    {
      use std::os::unix::fs::PermissionsExt;
      let md = std::fs::metadata(&build_env_path)?;
      let mode = md.permissions().mode() & 0o777;
      if mode & 0o077 != 0 {
        eprintln!(
          "warning: {} is group/world-readable (mode {:o}); consider chmod 600",
          &build_env_path.display(),
          mode
        );
      }
    }
    load_env_file_contents(&build_env_path)
      .with_context(|| format!("Failed to read {}", &build_env_path.display()))
  } else {
    Ok(HashMap::new())
  }
}
