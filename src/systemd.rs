use crate::log::{debug, log};
use crate::units_spec_builder::{render_and_write, UnitsSpec, WriteOutcome};
use anyhow::Result;
use std::fs;
use std::process::Stdio;
use tokio::process::Command;

/*
- app-<app>.target          A virtual “stack switch” for your app.
- app-<app>-acc.service     Accessories (Redis/Postgres) Compose project (<app>-acc).
- app-<app>-web.service     Web process (service name = "web") in Compose project <app>.
- app-<app>-worker.service  Worker process (service name = "worker") in Compose project <app>.
- All process units: Type=oneshot, RemainAfterExit=yes (Docker keeps containers running).
- Process units declare `After=app-<app>-acc.service` and `Wants=app-<app>-acc.service`
  when accessories exist; otherwise they just `After=docker.service network-online.target`.
 */

/// Clean up orphaned unit files for processes/accessories that no longer exist.
///
/// This function:
/// 1. Scans the systemd directory for unit files matching app-<app>-*.service
/// 2. Identifies orphaned services (not in current processes or accessories list)
/// 3. Stops and disables each orphaned service
/// 4. Deletes the orphaned unit file
/// 5. Logs all actions
async fn cleanup_orphaned_units_impl(
  app: &str,
  processes: &[String],
  accessories: &[String],
  systemd_dir: &std::path::Path,
) -> Result<()> {
  // Read directory entries
  let entries = match fs::read_dir(systemd_dir) {
    Ok(entries) => entries,
    Err(e) => {
      debug(&format!(
        "Could not read systemd directory {}: {}",
        systemd_dir.display(),
        e
      ));
      return Ok(());
    }
  };

  // Build set of expected unit files
  let mut expected_units = std::collections::HashSet::new();

  // Add accessories service if accessories exist
  if !accessories.is_empty() {
    expected_units.insert(format!("app-{}-acc.service", app));
  }

  // Add per-process services
  for proc in processes {
    expected_units.insert(format!("app-{}-{}.service", app, proc));
  }

  // Find orphaned service files
  let pattern = format!("app-{}-", app);
  for entry in entries.flatten() {
    let file_name = entry.file_name();
    let file_name_str = file_name.to_string_lossy();

    // Only consider service files matching our app pattern (exclude target files)
    if !file_name_str.starts_with(&pattern) || !file_name_str.ends_with(".service") {
      continue;
    }

    // Skip if this is an expected unit
    if expected_units.contains(file_name_str.as_ref()) {
      continue;
    }

    // Found an orphaned unit file
    let unit_name = file_name_str.to_string();
    let unit_path = entry.path();

    log(&format!("Found orphaned unit: {}", unit_name));

    // Stop the service (log warning if it fails, but don't propagate error)
    let _ = systemctl_status_ok(
      &["--user", "stop", &unit_name],
      Some(&format!("stop {}", unit_name)),
    )
    .await;

    // Disable the service (log warning if it fails, but don't propagate error)
    let _ = systemctl_status_ok(
      &["--user", "disable", &unit_name],
      Some(&format!("disable {}", unit_name)),
    )
    .await;

    // Delete the unit file
    debug(&format!("Deleting unit file: {}", unit_path.display()));
    if let Err(e) = fs::remove_file(&unit_path) {
      log(&format!(
        "Warning: Could not delete {}: {}",
        unit_path.display(),
        e
      ));
    } else {
      log(&format!("Deleted orphaned unit: {}", unit_name));
    }
  }

  Ok(())
}

/// Wrapper for cleanup_orphaned_units_impl that uses the default systemd directory
async fn cleanup_orphaned_units(
  app: &str,
  processes: &[String],
  accessories: &[String],
) -> Result<()> {
  let spec = UnitsSpec::builder(app)?.build();
  cleanup_orphaned_units_impl(app, processes, accessories, &spec.systemd_dir).await
}

/// Write systemd unit files for the given app, processes, and accessories.
/// This function first cleans up any orphaned units, then generates and writes
/// the necessary unit files based on the provided processes and accessories.
/// It logs the outcome of each write operation.
pub async fn write_unit(app: &str, processes: &[String], accessories: &[String]) -> Result<()> {
  // Clean up orphaned units before writing new ones
  cleanup_orphaned_units(app, processes, accessories).await?;

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
// When operation_desc is provided, logs warnings on failure.
async fn systemctl_status_ok(args: &[&str], operation_desc: Option<&str>) -> Result<bool> {
  let status = Command::new("systemctl").args(args).status().await;

  match status {
    Ok(s) if s.success() => {
      if let Some(desc) = operation_desc {
        debug(&format!("Successfully {}", desc));
      }
      Ok(true)
    }
    Ok(_) => {
      if let Some(desc) = operation_desc {
        log(&format!(
          "Warning: Failed to {} - may require manual intervention",
          desc
        ));
      }
      Ok(false)
    }
    Err(e) => {
      if let Some(desc) = operation_desc {
        log(&format!(
          "Warning: Error trying to {}: {} - this could indicate permission issues",
          desc, e
        ));
      }
      Err(e.into())
    }
  }
}

/// Reload unit files, then:
/// - if `unit` is active -> enable + restart (to pick up changes)
/// - else                -> enable --now (start if new/inactive, no extra bounce)
pub async fn apply_unit_changes(unit: &str) -> Result<()> {
  // 1) Make systemd read updated unit files
  reload_systemd_daemon().await?;
  // 2) Check if it's currently active
  let is_active = systemctl_status_ok(&["--user", "is-active", unit], None).await?;
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

#[cfg(test)]
mod tests {
  use super::*;
  use std::fs::File;
  use std::io::Write;
  use tempfile::TempDir;

  #[tokio::test]
  async fn test_cleanup_orphaned_units() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let systemd_dir = temp_dir.path().join("systemd");
    let app_dir = temp_dir.path().join("apps").join("testapp");

    // Create systemd directory
    fs::create_dir_all(&systemd_dir)?;
    fs::create_dir_all(&app_dir)?;

    // Create a mock UnitsSpec builder that returns our temp directories
    // We'll need to use the write_unit function which calls cleanup_orphaned_units

    // First, create some orphaned unit files
    let orphaned_files = vec![
      "app-testapp-oldworker.service",
      "app-testapp-deprecated.service",
      "app-testapp-removed.service",
    ];

    for file_name in &orphaned_files {
      let file_path = systemd_dir.join(file_name);
      let mut file = File::create(&file_path)?;
      writeln!(
        file,
        r#"[Unit]
Description=Orphaned service

[Service]
Type=oneshot
ExecStart=/bin/true

[Install]
WantedBy=default.target"#
      )?;
    }

    // Create a file that should NOT be cleaned up (different app)
    let other_app_file = systemd_dir.join("app-otherapp-web.service");
    let mut file = File::create(&other_app_file)?;
    writeln!(
      file,
      r#"[Unit]
Description=Other app service

[Service]
Type=oneshot
ExecStart=/bin/true"#
    )?;

    // Create a file that should NOT be cleaned up (target file)
    let target_file = systemd_dir.join("app-testapp.target");
    let mut file = File::create(&target_file)?;
    writeln!(
      file,
      r#"[Unit]
Description=Test app target"#
    )?;

    // Verify orphaned files exist
    for file_name in &orphaned_files {
      assert!(
        systemd_dir.join(file_name).exists(),
        "Orphaned file {} should exist before cleanup",
        file_name
      );
    }
    assert!(
      other_app_file.exists(),
      "Other app file should exist before cleanup"
    );
    assert!(target_file.exists(), "Target file should exist");

    // Now call write_unit with current processes (web, worker)
    // This should clean up the orphaned units but keep the active ones
    let processes = vec!["web".to_string(), "worker".to_string()];
    let accessories = vec!["postgres".to_string()];

    // We need to temporarily override the systemd_dir for this test
    // Since we can't easily mock the UnitsSpec::builder, we'll test cleanup_orphaned_units directly
    // For this, we need to make it pub(crate) or test it through write_unit

    // Call cleanup_orphaned_units_impl with our test directory
    cleanup_orphaned_units_impl("testapp", &processes, &accessories, &systemd_dir).await?;

    // Verify orphaned files are deleted
    for file_name in &orphaned_files {
      assert!(
        !systemd_dir.join(file_name).exists(),
        "Orphaned file {} should be deleted",
        file_name
      );
    }

    // Verify other app file still exists (different app prefix)
    assert!(
      other_app_file.exists(),
      "Other app file should NOT be deleted"
    );

    // Verify target file still exists (not a .service file)
    assert!(target_file.exists(), "Target file should NOT be deleted");

    Ok(())
  }

  #[tokio::test]
  async fn test_cleanup_orphaned_units_with_accessories() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let systemd_dir = temp_dir.path().join("systemd");

    fs::create_dir_all(&systemd_dir)?;

    // Create an orphaned accessories service (when accessories list is now empty)
    let orphaned_acc = systemd_dir.join("app-testapp-acc.service");
    let mut file = File::create(&orphaned_acc)?;
    writeln!(
      file,
      r#"[Unit]
Description=Orphaned accessories

[Service]
Type=oneshot
ExecStart=/bin/true"#
    )?;

    // Create an orphaned process service
    let orphaned_proc = systemd_dir.join("app-testapp-oldproc.service");
    let mut file = File::create(&orphaned_proc)?;
    writeln!(
      file,
      r#"[Unit]
Description=Orphaned process

[Service]
Type=oneshot
ExecStart=/bin/true"#
    )?;

    assert!(orphaned_acc.exists(), "Orphaned acc should exist");
    assert!(orphaned_proc.exists(), "Orphaned proc should exist");

    // Call cleanup with NO accessories and only "web" process
    // This should delete both the acc service and oldproc service
    let processes = vec!["web".to_string()];
    let accessories: Vec<String> = vec![];

    cleanup_orphaned_units_impl("testapp", &processes, &accessories, &systemd_dir).await?;

    // Verify both are deleted
    assert!(
      !orphaned_acc.exists(),
      "Orphaned accessories service should be deleted when accessories list is empty"
    );
    assert!(
      !orphaned_proc.exists(),
      "Orphaned process service should be deleted"
    );

    Ok(())
  }

  #[tokio::test]
  async fn test_cleanup_orphaned_units_preserves_current_units() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let systemd_dir = temp_dir.path().join("systemd");

    fs::create_dir_all(&systemd_dir)?;

    // Create unit files that match current processes and accessories
    let current_web = systemd_dir.join("app-myapp-web.service");
    let current_worker = systemd_dir.join("app-myapp-worker.service");
    let current_acc = systemd_dir.join("app-myapp-acc.service");

    for path in &[&current_web, &current_worker, &current_acc] {
      let mut file = File::create(path)?;
      writeln!(
        file,
        r#"[Unit]
Description=Current service

[Service]
Type=oneshot
ExecStart=/bin/true"#
      )?;
    }

    // Create an orphaned file
    let orphaned = systemd_dir.join("app-myapp-deprecated.service");
    let mut file = File::create(&orphaned)?;
    writeln!(
      file,
      r#"[Unit]
Description=Orphaned service

[Service]
Type=oneshot
ExecStart=/bin/true"#
    )?;

    // Call cleanup with current processes and accessories
    let processes = vec!["web".to_string(), "worker".to_string()];
    let accessories = vec!["postgres".to_string()];

    cleanup_orphaned_units_impl("myapp", &processes, &accessories, &systemd_dir).await?;

    // Verify current units are preserved
    assert!(
      current_web.exists(),
      "Current web service should be preserved"
    );
    assert!(
      current_worker.exists(),
      "Current worker service should be preserved"
    );
    assert!(
      current_acc.exists(),
      "Current accessories service should be preserved"
    );

    // Verify orphaned is deleted
    assert!(!orphaned.exists(), "Orphaned service should be deleted");

    Ok(())
  }

  #[tokio::test]
  async fn test_cleanup_orphaned_units_handles_missing_directory() -> Result<()> {
    // This should not error even though the systemd directory doesn't exist
    // (cleanup_orphaned_units will try to read a non-existent dir based on the app config)
    let processes = vec!["web".to_string()];
    let accessories: Vec<String> = vec![];

    // Should complete successfully without panicking
    let result = cleanup_orphaned_units("testapp", &processes, &accessories).await;

    assert!(
      result.is_ok(),
      "Cleanup should handle missing directory gracefully"
    );

    Ok(())
  }
}
