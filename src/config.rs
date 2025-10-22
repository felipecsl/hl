use crate::log::debug;
use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;
use tokio::fs;

pub fn hl_root() -> PathBuf {
  // Allow overriding for tests
  if let Ok(override_root) = std::env::var("HL_ROOT_OVERRIDE") {
    return PathBuf::from(override_root);
  }
  home_dir().join("hl").join("apps")
}

pub fn home_dir() -> PathBuf {
  let home = std::env::var("HOME").expect("HOME environment variable not set");
  PathBuf::from(home)
}

pub fn hl_git_root(app: &str) -> PathBuf {
  home_dir()
    .join("hl")
    .join("git")
    .join(format!("{}.git", app))
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct HLConfig {
  pub app: String,
  pub image: String,
  pub domain: String,
  pub service_port: u16,
  #[serde(default = "default_resolver")]
  pub resolver: String,
  #[serde(default = "default_network")]
  pub network: String,
  #[serde(default = "default_platforms")]
  pub platforms: String,
  pub health: HealthConfig,
  #[serde(default)]
  pub migrations: MigrationsConfig,
  #[serde(default)]
  pub secrets: Vec<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct HealthConfig {
  pub url: String,
  #[serde(default = "default_interval")]
  pub interval: String,
  #[serde(default = "default_timeout")]
  pub timeout: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct MigrationsConfig {
  #[serde(default = "default_migration_command")]
  pub command: Vec<String>,
  #[serde(default)]
  pub env: HashMap<String, String>,
}

fn default_resolver() -> String {
  "myresolver".to_string()
}

fn default_network() -> String {
  "traefik_proxy".to_string()
}

fn default_platforms() -> String {
  "linux/amd64".to_string()
}

fn default_interval() -> String {
  "2s".to_string()
}

fn default_timeout() -> String {
  "45s".to_string()
}

fn default_migration_command() -> Vec<String> {
  vec!["bin/rails".to_string(), "db:migrate".to_string()]
}

impl Default for MigrationsConfig {
  fn default() -> Self {
    Self {
      command: default_migration_command(),
      env: HashMap::new(),
    }
  }
}

pub async fn load_config(app: &str) -> Result<HLConfig> {
  let path = app_dir(app).join("hl.yml");
  debug(&format!("loading config from: {}", path.display()));

  if !path.exists() {
    anyhow::bail!("Config file not found at: {}", path.display());
  }

  let content = fs::read_to_string(&path)
    .await
    .context(format!("Failed to read config file: {}", path.display()))?;

  let config: HLConfig = serde_yaml::from_str(&content)
    .context(format!("Failed to parse config file: {}", path.display()))?;

  debug(&format!(
    "successfully loaded config for app: {}",
    config.app
  ));

  Ok(config)
}

pub fn app_dir(app: &str) -> PathBuf {
  hl_root().join(app)
}

/// Returns the path to the runtime environment file for the given app.
pub fn env_file(app: &str) -> PathBuf {
  app_dir(app).join(".env")
}

/// Returns the path to the build environment file for the given app.
pub fn build_env_file(app: &str) -> PathBuf {
  app_dir(app).join(".env.build")
}

pub fn systemd_dir() -> PathBuf {
  home_dir().join(".config/systemd/user")
}

/// Parse duration strings like "2s", "45s", "100ms" into milliseconds
pub fn parse_duration(s: &str) -> Result<u64> {
  let re = regex::Regex::new(r"^(\d+)(ms|s|m)$")?;
  let caps = re
    .captures(s)
    .ok_or_else(|| anyhow::anyhow!("bad duration: {}", s))?;

  let n: u64 = caps[1].parse()?;
  let unit = &caps[2];

  let ms = match unit {
    "ms" => n,
    "s" => n * 1000,
    "m" => n * 60_000,
    _ => anyhow::bail!("bad duration: {}", s),
  };

  Ok(ms)
}
