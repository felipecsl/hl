use crate::log::debug;
use crate::units_spec_builder::{render_and_write, UnitsSpec, WriteOutcome};
use anyhow::Result;
use std::process::Stdio;
use tokio::process::Command;

pub async fn write_unit(
    app: &str,
    processes: &[String],
    accessories: &[String],
) -> Result<()> {
    let spec_builder = UnitsSpec::builder(app)?;
    let spec = spec_builder
        .processes(processes.to_vec())
        .accessories(accessories.to_vec())
        .build();
    let outcomes = render_and_write(&spec)?;
    for o in outcomes {
        match o {
            WriteOutcome::Created(p) => debug(&format!("Created {}", p.display())),
            WriteOutcome::Updated(p) => debug(&format!("Updated {}", p.display())),
            WriteOutcome::Unchanged(p) => debug(&format!("Unchanged {}", p.display())),
        }
    }

    debug("reloading systemd daemon");

    reload_systemd().await?;

    debug("systemd unit written and daemon reloaded successfully");

    Ok(())
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

pub async fn reload_systemd() -> Result<()> {
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
    Ok(())
}
