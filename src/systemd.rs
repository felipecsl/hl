use crate::log::debug;
use crate::units_spec_builder::{render_and_write, UnitsSpec, WriteOutcome};
use anyhow::Result;
use std::process::Stdio;
use tokio::process::Command;

/*
                                   ┌───────────────────────────────┐
                                   │  multi-user.target            │
                                   └──────────────┬────────────────┘
                                                  │ Wants
                                     enable →     ▼
                                   ┌───────────────────────────────┐
                                   │  app-<app>.target             │
                                   └───────┬───────────┬───────────┘
                                           │Wants      │Wants
                                           │           │
                                           ▼           ▼
                      ┌──────────────────────────┐   ┌──────────────────────────┐
                      │ app-<app>-acc.service    │   │ app-<app>-web.service    │
                      └───────────┬──────────────┘   └───────────┬──────────────┘
                                  │ After/Requires                │ After + Wants acc
                                  │ docker.service                │
                                  │ network-online.target         │
                                  │                               │
                                  ▼                               ▼
                  (docker compose -p <app>-acc up -d acc…)   (docker compose -p <app> up -d web)
                                                                ↑
                                                                │
                      ┌──────────────────────────┐              │
                      │ app-<app>-worker.service │◄─────────────┘
                      └───────────┬──────────────┘
                                  │ After + Wants acc
                                  │ (optional ExecStartPost: --scale worker=N)
                                  ▼
                           (docker compose -p <app> up -d worker)



Legend:
- app-<app>.target          A virtual “stack switch” for your app.
- app-<app>-acc.service     Accessories (Redis/Postgres) Compose project (<app>-acc).
- app-<app>-web.service     Web process (service name = "web") in Compose project <app>.
- app-<app>-worker.service  Worker process (service name = "worker") in Compose project <app>.
- All process units: Type=oneshot, RemainAfterExit=yes (Docker keeps containers running).
- Process units declare `After=app-<app>-acc.service` and `Wants=app-<app>-acc.service`
  when accessories exist; otherwise they just `After=docker.service network-online.target`.
 */

pub async fn write_unit(app: &str, processes: &[String], accessories: &[String]) -> Result<()> {
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

    Ok(())
}

pub async fn enable_accessories(app: &str) -> Result<()> {
    let unit = format!("app-{}-acc.service", app);

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

pub async fn restart_app_target(app: &str) -> Result<()> {
    let unit = format!("app-{}.target", app);

    debug(&format!("restarting systemd service: {}", unit));

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

pub async fn reload_systemd_daemon() -> Result<()> {
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
