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
    systemctl_cmd(&["--user", "enable", "--now", &unit]).await
}

pub async fn restart_accessories(app: &str) -> Result<()> {
    let unit = format!("app-{}-acc.service", app);
    debug(&format!("restarting systemd service: {}", unit));
    systemctl_cmd(&["--user", "restart", &unit]).await
}

pub async fn start_accessories(app: &str) -> Result<()> {
    let unit = format!("app-{}-acc.service", app);
    debug(&format!("starting systemd service: {}", unit));
    systemctl_cmd(&["--user", "start", &unit]).await
}

pub async fn restart_app_target(app: &str) -> Result<()> {
    let unit = format!("app-{}.target", app);
    debug(&format!("restarting systemd service: {}", unit));
    systemctl_cmd(&["--user", "restart", &unit]).await
}

pub async fn reload_systemd_daemon() -> Result<()> {
    systemctl_cmd(&["--user", "daemon-reload"]).await
}

pub async fn stop_disable_app_target(app: &str) -> Result<()> {
    let unit = format!("app-{}.target", app);
    debug(&format!("stopping and disabling systemd target: {}", unit));
    systemctl_cmd(&["--user", "stop", &unit]).await?;
    systemctl_cmd(&["--user", "disable", &unit]).await?;

    Ok(())
}

// Lightweight status check that does NOT error on non-zero exit.
async fn systemctl_status_ok(args: &[&str]) -> Result<bool> {
    let status = Command::new("systemctl").args(args).status().await?;
    Ok(status.success())
}

/// Reload unit files, then:
/// - if `unit` is active -> enable + restart (to pick up changes)
/// - else                -> enable --now (start if new/inactive, no extra bounce)
pub async fn apply_unit_changes(unit: &str) -> Result<()> {
    // 1) Make systemd read updated unit files
    reload_systemd_daemon().await?;
    // 2) Check if it's currently active
    let is_active = systemctl_status_ok(&["--user", "is-active", unit]).await?;
    if is_active {
        // Known & running: ensure enabled, then restart to apply new unit config
        systemctl_cmd(&["--user", "enable", unit]).await?;
        systemctl_cmd(&["--user", "restart", unit]).await?;
    } else {
        // New or stopped: enable and start once (no redundant restart)
        systemctl_cmd(&["--user", "enable", "--now", unit]).await?;
    }

    Ok(())
}

async fn systemctl_cmd(args: &[&str]) -> Result<()> {
    let status = Command::new("systemctl")
        .args(args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .await?;

    if !status.success() {
        anyhow::bail!("systemctl --user {:?} failed with status: {}", args, status);
    }

    Ok(())
}
