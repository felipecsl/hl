use crate::config::app_dir;
use anyhow::Result;
use std::env;
use std::process::Stdio;
use tokio::fs;
use tokio::process::Command;

pub async fn write_unit(app: &str) -> Result<String> {
    let unit = format!("app-{}.service", app);
    let wd = app_dir(app);

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
ExecStart=/usr/bin/docker compose -f compose.yml up -d
ExecStop=/usr/bin/docker compose -f compose.yml down
TimeoutStartSec=0

[Install]
WantedBy=default.target
"#,
        app = app,
        wd = wd.display()
    );

    // Get the user's home directory
    let home = env::var("HOME").or_else(|_| {
        env::var("USERPROFILE") // Windows fallback
    })?;

    let systemd_user_dir = format!("{}/.config/systemd/user", home);

    // Ensure the directory exists
    fs::create_dir_all(&systemd_user_dir).await?;

    let unit_path = format!("{}/{}", systemd_user_dir, unit);
    fs::write(&unit_path, text).await?;

    // Reload systemd daemon
    let status = Command::new("systemctl")
        .args(["--user", "daemon-reload"])
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .await?;

    if !status.success() {
        anyhow::bail!("systemctl --user daemon-reload failed");
    }

    Ok(unit)
}

pub async fn enable_service(app: &str) -> Result<()> {
    let unit = format!("app-{}.service", app);

    // Enable and start the service (idempotent operation)
    let status = Command::new("systemctl")
        .args(["--user", "enable", "--now", &unit])
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .await?;

    if !status.success() {
        anyhow::bail!("systemctl --user enable failed");
    }

    Ok(())
}
