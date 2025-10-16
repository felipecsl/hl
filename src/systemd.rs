use crate::config::app_dir;
use anyhow::Result;
use std::process::Stdio;
use tokio::fs;
use tokio::process::Command;

pub async fn write_unit(app: &str) -> Result<String> {
    let unit = format!("app-{}.service", app);
    let wd = app_dir(app);

    let text = format!(
        r#"[Unit]
Description=Compose stack for {app}
After=docker.service
Requires=docker.service

[Service]
Type=oneshot
RemainAfterExit=yes
WorkingDirectory={wd}
ExecStart=/usr/bin/docker compose -f compose.yml up -d
ExecStop=/usr/bin/docker compose -f compose.yml down
TimeoutStartSec=0

[Install]
WantedBy=multi-user.target
"#,
        app = app,
        wd = wd.display()
    );

    let unit_path = format!("/etc/systemd/system/{}", unit);
    fs::write(&unit_path, text).await?;

    // Reload systemd daemon
    let status = Command::new("systemctl")
        .args(["daemon-reload"])
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .await?;

    if !status.success() {
        anyhow::bail!("systemctl daemon-reload failed");
    }

    // Enable and start the service
    let status = Command::new("systemctl")
        .args(["enable", "--now", &unit])
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .await?;

    if !status.success() {
        anyhow::bail!("systemctl enable failed");
    }

    Ok(unit)
}
