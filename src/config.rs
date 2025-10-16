use anyhow::Result;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;
use tokio::fs;

pub const HL_ROOT: &str = "/home/felipecsl/prj/apps";

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
    let path = PathBuf::from(HL_ROOT).join(app).join("homelab.yml");
    let content = fs::read_to_string(&path).await?;
    let config: HLConfig = serde_yaml::from_str(&content)?;
    Ok(config)
}

pub fn app_dir(app: &str) -> PathBuf {
    PathBuf::from(HL_ROOT).join(app)
}

pub fn env_file(app: &str) -> PathBuf {
    app_dir(app).join(".env")
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
