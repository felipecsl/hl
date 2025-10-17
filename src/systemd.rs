use crate::config::app_dir;
use crate::log::debug;
use anyhow::Result;
use std::env;
use std::process::Stdio;
use tokio::fs;
use tokio::process::Command;

pub async fn write_unit(app: &str) -> Result<String> {
    let unit = format!("app-{}.service", app);
    let wd = app_dir(app);

    debug(&format!(
        "write_unit: app={}, unit={}, working_directory={}",
        app,
        unit,
        wd.display()
    ));

    if !wd.exists() {
        anyhow::bail!("App directory not found: {}", wd.display());
    }

    // Build the compose file list
    let mut compose_files = vec!["compose.yml".to_string()];

    // Check for compose.postgres.yml
    let postgres_compose = wd.join("compose.postgres.yml");
    if postgres_compose.exists() {
        compose_files.push("compose.postgres.yml".to_string());
    }

    // Build the docker compose command arguments
    let compose_args = compose_files
        .iter()
        .map(|f| format!("-f {}", f))
        .collect::<Vec<_>>()
        .join(" ");

    let text = format!(
        r#"[Unit]
Description=Compose stack for {app}
After=default.target

[Service]
Type=oneshot
RemainAfterExit=yes
WorkingDirectory={wd}
# Wait until the Docker daemon/socket is up
ExecStartPre=/bin/sh -c 'until /usr/bin/docker info >/dev/null 2>&1; do sleep 1; done'
ExecStart=/usr/bin/docker compose {compose_args} up -d
ExecStop=/usr/bin/docker compose {compose_args} down
TimeoutStartSec=0

[Install]
WantedBy=default.target
"#,
        app = app,
        wd = wd.display(),
        compose_args = compose_args
    );

    // Get the user's home directory
    let home = env::var("HOME").or_else(|_| {
        env::var("USERPROFILE") // Windows fallback
    })?;

    let systemd_user_dir = format!("{}/.config/systemd/user", home);

    debug(&format!("systemd user directory: {}", systemd_user_dir));

    // Ensure the directory exists
    fs::create_dir_all(&systemd_user_dir).await?;

    let unit_path = format!("{}/{}", systemd_user_dir, unit);

    debug(&format!("writing systemd unit file to: {}", unit_path));

    fs::write(&unit_path, text).await?;

    debug("reloading systemd daemon");

    // Reload systemd daemon
    let status = Command::new("systemctl")
        .args(["--user", "daemon-reload"])
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .await?;

    if !status.success() {
        anyhow::bail!(
            "systemctl --user daemon-reload failed with status: {}",
            status
        );
    }

    debug("systemd unit written and daemon reloaded successfully");

    Ok(unit)
}

pub async fn enable_service(app: &str) -> Result<()> {
    let unit = format!("app-{}.service", app);

    debug(&format!("enabling systemd service: {}", unit));

    // Enable and start the service (idempotent operation)
    let status = Command::new("systemctl")
        .args(["--user", "enable", "--now", &unit])
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .await?;

    if !status.success() {
        anyhow::bail!("systemctl --user enable failed with status: {}", status);
    }

    debug("systemd service enabled successfully");

    Ok(())
}

pub async fn restart_service(app: &str) -> Result<()> {
    let unit = format!("app-{}.service", app);

    debug(&format!("restarting systemd service: {}", unit));

    // Restart the service
    let status = Command::new("systemctl")
        .args(["--user", "restart", &unit])
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .await?;

    if !status.success() {
        anyhow::bail!("systemctl --user restart failed with status: {}", status);
    }

    debug("systemd service restarted successfully");

    Ok(())
}