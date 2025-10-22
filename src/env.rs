use anyhow::{Context, Result};
use std::collections::HashMap;

use crate::config::build_env_file;

/// Read environment variable key-value pairs from a .env (or .env.build) file
/// # Arguments
/// * `file_path` - Path to the .env file
/// # Returns
/// HashMap mapping environment variable names to their values
pub fn load_env_file_contents(path: &std::path::Path) -> Result<HashMap<String, String>> {
  let mut map = HashMap::new();
  for item in dotenvy::from_path_iter(path)? {
    let (k, v) = item?;
    map.insert(k, v);
  }
  Ok(map)
}

/// Load build environment variables for the given app
/// # Arguments
/// * `app` - Application name
/// # Returns
/// HashMap mapping environment variable names to their values
/// # Notes
/// If the build environment file does not exist, returns an empty map
pub fn load_build_env_contents(app: &str) -> Result<HashMap<String, String>> {
  let build_env_path = build_env_file(app).display().to_string();
  return if std::path::Path::new(&build_env_path).exists() {
    // Optional: warn if perms are too loose
    #[cfg(unix)]
    {
      use std::os::unix::fs::PermissionsExt;
      let md = std::fs::metadata(&build_env_path)?;
      let mode = md.permissions().mode() & 0o777;
      if mode & 0o077 != 0 {
        eprintln!(
          "warning: {} is group/world-readable (mode {:o}); consider chmod 600",
          &build_env_path, mode
        );
      }
    }
    load_env_file_contents(std::path::Path::new(&build_env_path))
      .with_context(|| format!("Failed to read {}", &build_env_path))
  } else {
    Ok(HashMap::new())
  };
}
