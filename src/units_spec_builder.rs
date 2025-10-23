use anyhow::Result;
use std::fmt::Write as _;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use crate::config::{app_dir, systemd_dir};

#[derive(Debug, Clone)]
pub struct UnitsSpec {
  pub app_name: String,
  pub processes: Vec<String>,   // e.g. ["web","worker"]
  pub accessories: Vec<String>, // e.g. ["postgres","redis"]
  /// Where to drop unit files, typically /etc/systemd/system
  pub systemd_dir: PathBuf,
  /// App runtime dir, e.g. /srv/myapp
  pub app_dir: PathBuf,
  /// Optional environment file for worker scaling, etc. (e.g., /etc/default/app-myapp)
  pub env_file: Option<PathBuf>,
}

impl UnitsSpec {
  pub fn builder(app_name: &str) -> Result<UnitsSpecBuilder> {
    Ok(UnitsSpecBuilder {
      app_name: app_name.into(),
      processes: vec![],
      accessories: vec![],
      systemd_dir: systemd_dir(),
      app_dir: app_dir(app_name),
      env_file: app_dir(app_name).join(".env").into(),
    })
  }
}

pub struct UnitsSpecBuilder {
  app_name: String,
  processes: Vec<String>,
  accessories: Vec<String>,
  systemd_dir: PathBuf,
  app_dir: PathBuf,
  env_file: Option<PathBuf>,
}

impl UnitsSpecBuilder {
  pub fn processes(mut self, procs: impl Into<Vec<String>>) -> Self {
    self.processes = procs.into();
    self
  }
  pub fn accessories(mut self, accs: impl Into<Vec<String>>) -> Self {
    self.accessories = accs.into();
    self
  }
  pub fn build(self) -> UnitsSpec {
    UnitsSpec {
      app_name: self.app_name,
      processes: self.processes,
      accessories: self.accessories,
      systemd_dir: self.systemd_dir,
      app_dir: self.app_dir,
      env_file: self.env_file,
    }
  }
}

#[derive(Debug)]
pub enum WriteOutcome {
  Created(PathBuf),
  Updated(PathBuf),
  Unchanged(PathBuf),
}

pub fn render_and_write(spec: &UnitsSpec) -> std::io::Result<Vec<WriteOutcome>> {
  fs::create_dir_all(&spec.systemd_dir)?;

  let mut outcomes = Vec::new();

  // 1) Target
  let target_name = format!("app-{}.target", spec.app_name);
  let target_path = spec.systemd_dir.join(&target_name);
  let target_content = render_target(
    &spec.app_name,
    &spec.processes,
    !spec.accessories.is_empty(),
  );
  outcomes.push(write_if_changed(&target_path, &target_content)?);

  // 2) Accessories service (only if accessories exist)
  if !spec.accessories.is_empty() {
    let acc_name = format!("app-{}-acc.service", spec.app_name);
    let acc_path = spec.systemd_dir.join(&acc_name);
    let acc_content = render_accessories_service(spec);
    outcomes.push(write_if_changed(&acc_path, &acc_content)?);
  }

  // 3) Per-process services
  for proc_name in &spec.processes {
    let svc_name = format!("app-{}-{}.service", spec.app_name, proc_name);
    let svc_path = spec.systemd_dir.join(&svc_name);
    let svc_content = render_process_service(spec, proc_name);
    outcomes.push(write_if_changed(&svc_path, &svc_content)?);
  }

  Ok(outcomes)
}

fn write_if_changed(path: &Path, desired: &str) -> std::io::Result<WriteOutcome> {
  // Read existing (if any)
  let mut existing = String::new();
  if let Ok(mut f) = File::open(path) {
    f.read_to_string(&mut existing).ok();
  }

  if normalize(&existing) == normalize(desired) {
    return Ok(WriteOutcome::Unchanged(path.to_path_buf()));
  }

  // Write atomically: .tmp then rename
  if let Some(parent) = path.parent() {
    fs::create_dir_all(parent)?;
  }
  let tmp_path = path.with_extension("tmp");
  {
    let mut f = File::create(&tmp_path)?;
    f.write_all(desired.as_bytes())?;
    f.sync_all()?;
  }
  fs::rename(&tmp_path, path)?;

  if existing.is_empty() {
    Ok(WriteOutcome::Created(path.to_path_buf()))
  } else {
    Ok(WriteOutcome::Updated(path.to_path_buf()))
  }
}

fn normalize(s: &str) -> String {
  // Trim trailing whitespace to avoid noisy diffs
  s.lines()
    .map(|l| l.trim_end())
    .collect::<Vec<_>>()
    .join("\n")
}

fn render_target(app: &str, processes: &[String], has_acc: bool) -> String {
  let mut wants = Vec::new();
  if has_acc {
    wants.push(format!("app-{}-acc.service", app));
  }
  for p in processes {
    wants.push(format!("app-{}-{}.service", app, p));
  }
  let mut unit = String::new();
  writeln!(
    &mut unit,
    r#"[Unit]
Description=App {app} stack
After=default.target
Wants={wants}
"#,
    app = app,
    wants = wants.join(" ")
  )
  .unwrap();
  writeln!(
    &mut unit,
    r#"[Install]
WantedBy=default.target"#
  )
  .unwrap();
  unit
}

fn render_accessories_service(spec: &UnitsSpec) -> String {
  let app = &spec.app_name;
  let app_dir = &spec.app_dir;
  let project = format!("{app}-acc");
  let base = app_dir.join("compose.yml");
  // Turn ["postgres","redis"] into "/srv/app/compose.postgres.yml:/srv/app/compose.redis.yml"
  let acc_files = spec
    .accessories
    .iter()
    .map(|a| {
      format!(
        "  -f {}",
        app_dir.join(format!("compose.{a}.yml")).display()
      )
    })
    .collect::<Vec<_>>()
    .join(" \\\n");

  let mut body = String::new();
  writeln!(
        &mut body,
        r#"[Unit]
Description=App {app} accessories (Redis/Postgres/etc.)
After=default.target
PartOf=app-{app}.target

[Service]
Type=oneshot
RemainAfterExit=yes
ExecStartPre=/usr/bin/bash -lc 'for i in {{1..30}}; do docker version >/dev/null 2>&1 && exit 0; sleep 1; done; echo "Docker unavailable" >&2; exit 1'
WorkingDirectory={app_dir}
ExecStart=/usr/bin/docker compose -p {project} \
  -f {base} \
{accessories} \
  up -d
ExecStop=/usr/bin/docker compose -p {project} \
  -f {base} \
{accessories} \
  stop
Restart=no

[Install]
WantedBy=default.target
"#,
        app = app,
        project = project,
        base = base.display(),
        app_dir = app_dir.display(),
        accessories = acc_files
    )
    .unwrap();
  body
}

fn render_process_service(spec: &UnitsSpec, proc_name: &str) -> String {
  let app = &spec.app_name;
  let app_dir = &spec.app_dir;
  let project = app; // app project
  let base = app_dir.join("compose.yml");
  let overlay = app_dir.join(format!("compose.{proc}.yml", proc = proc_name));
  let mut after = vec!["default.target".to_string()];
  let mut wants = Vec::new();
  if !spec.accessories.is_empty() {
    after.push(format!("app-{}-acc.service", spec.app_name));
    wants.push(format!("app-{}-acc.service", spec.app_name));
  }

  // Order: require accessories if any
  let mut unit = String::new();
  writeln!(
    &mut unit,
    r#"[Unit]
Description=App {app} {proc} process
After={after}
Wants={wants}
PartOf=app-{app}.target
"#,
    app = app,
    proc = proc_name,
    after = after.join(" "),
    wants = if wants.is_empty() {
      "".into()
    } else {
      wants.join(" ")
    },
  )
  .unwrap();

  writeln!(&mut unit, r#"[Service]"#).unwrap();
  writeln!(&mut unit, "Type=oneshot").unwrap();
  writeln!(&mut unit, "RemainAfterExit=yes").unwrap();
  writeln!(&mut unit, r#"ExecStartPre=/usr/bin/bash -lc 'for i in {{1..30}}; do docker version >/dev/null 2>&1 && exit 0; sleep 1; done; echo "Docker unavailable" >&2; exit 1'"#).unwrap();
  writeln!(
    &mut unit,
    "Environment=PROJECT_NAME={}",
    systemd_escape(project)
  )
  .unwrap();
  writeln!(&mut unit, "Environment=COMPOSE_BASE={}", base.display()).unwrap();
  writeln!(
    &mut unit,
    "Environment=COMPOSE_OVERLAYS={}",
    overlay.display()
  )
  .unwrap();
  if let Some(env_file) = &spec.env_file {
    writeln!(&mut unit, "EnvironmentFile=-{}", env_file.display()).unwrap();
  }
  writeln!(&mut unit, "WorkingDirectory={}", app_dir.display()).unwrap();

  // Start only the named service; scale (optional) for worker
  writeln!(
        &mut unit,
        "ExecStart=/usr/bin/docker compose -p ${{PROJECT_NAME}} -f ${{COMPOSE_BASE}} -f ${{COMPOSE_OVERLAYS}} up -d {svc} --remove-orphans",
        svc = proc_name
    ).unwrap();

  // Optional post-scale: only meaningful if you put WORKER_SCALE into env_file and this is "worker"
  if proc_name == "worker" {
    writeln!(
            &mut unit,
            "ExecStartPost=/usr/bin/docker compose -p ${{PROJECT_NAME}} -f ${{COMPOSE_BASE}} -f ${{COMPOSE_OVERLAYS}} up -d --scale {svc}=${{WORKER_SCALE:-1}} {svc}",
            svc = proc_name
        ).unwrap();
  }

  writeln!(
        &mut unit,
        "ExecStop=/usr/bin/docker compose -p ${{PROJECT_NAME}} -f ${{COMPOSE_BASE}} -f ${{COMPOSE_OVERLAYS}} stop {svc}",
        svc = proc_name
    )
    .unwrap();
  writeln!(&mut unit, "Restart=no").unwrap();

  writeln!(
    &mut unit,
    r#"
[Install]
WantedBy=app-{}.target"#,
    app
  )
  .unwrap();

  unit
}

/// Minimal escaping helper for Environment= values (spaces are rare, but be safe).
fn systemd_escape(s: &str) -> String {
  // systemd is forgiving here; we'll just avoid raw newlines and quotes.
  s.replace('"', r#"\""#).replace('\n', " ")
}

#[cfg(test)]
mod tests {
  use super::*;
  use tempfile::TempDir;

  #[test]
  fn test_render_and_write_complete_spec() -> std::io::Result<()> {
    let temp_dir = TempDir::new()?;
    let systemd_dir = temp_dir.path().join("systemd");
    let app_dir = temp_dir.path().join("apps").join("testapp");
    let app_dir_str = app_dir.display().to_string();

    let spec = UnitsSpec {
      app_name: "testapp".to_string(),
      processes: vec!["web".to_string(), "worker".to_string()],
      accessories: vec!["postgres".to_string(), "redis".to_string()],
      systemd_dir: systemd_dir.clone(),
      app_dir: app_dir.clone(),
      env_file: Some(app_dir.join(".env")),
    };

    let outcomes = render_and_write(&spec)?;

    // Should create 4 files: target, accessories service, web service, worker service
    assert_eq!(outcomes.len(), 4, "Should create 4 unit files");

    // Verify all outcomes are Created
    for outcome in &outcomes {
      match outcome {
        WriteOutcome::Created(_) => {}
        _ => panic!("Expected all files to be Created on first write"),
      }
    }

    // 1. Verify target file
    let target_path = systemd_dir.join("app-testapp.target");
    assert!(target_path.exists(), "Target file should exist");
    let target_content = fs::read_to_string(&target_path)?;
    let expected_target = r#"[Unit]
Description=App testapp stack
After=default.target
Wants=app-testapp-acc.service app-testapp-web.service app-testapp-worker.service

[Install]
WantedBy=default.target
"#;
    assert_eq!(target_content, expected_target);

    // 2. Verify accessories service
    let acc_path = systemd_dir.join("app-testapp-acc.service");
    assert!(acc_path.exists(), "Accessories service should exist");
    let acc_content = fs::read_to_string(&acc_path)?;
    let expected_acc = format!(
      r#"[Unit]
Description=App testapp accessories (Redis/Postgres/etc.)
After=default.target
PartOf=app-testapp.target

[Service]
Type=oneshot
RemainAfterExit=yes
ExecStartPre=/usr/bin/bash -lc 'for i in {{1..30}}; do docker version >/dev/null 2>&1 && exit 0; sleep 1; done; echo "Docker unavailable" >&2; exit 1'
WorkingDirectory={app_dir}
ExecStart=/usr/bin/docker compose -p testapp-acc \
  -f {app_dir}/compose.yml \
  -f {app_dir}/compose.postgres.yml \
  -f {app_dir}/compose.redis.yml \
  up -d
ExecStop=/usr/bin/docker compose -p testapp-acc \
  -f {app_dir}/compose.yml \
  -f {app_dir}/compose.postgres.yml \
  -f {app_dir}/compose.redis.yml \
  stop
Restart=no

[Install]
WantedBy=default.target

"#,
      app_dir = app_dir_str
    );
    assert_eq!(acc_content, expected_acc);

    // 3. Verify web service
    let web_path = systemd_dir.join("app-testapp-web.service");
    assert!(web_path.exists(), "Web service should exist");
    let web_content = fs::read_to_string(&web_path)?;
    let expected_web = format!(
      r#"[Unit]
Description=App testapp web process
After=default.target app-testapp-acc.service
Wants=app-testapp-acc.service
PartOf=app-testapp.target

[Service]
Type=oneshot
RemainAfterExit=yes
ExecStartPre=/usr/bin/bash -lc 'for i in {{1..30}}; do docker version >/dev/null 2>&1 && exit 0; sleep 1; done; echo "Docker unavailable" >&2; exit 1'
Environment=PROJECT_NAME=testapp
Environment=COMPOSE_BASE={app_dir}/compose.yml
Environment=COMPOSE_OVERLAYS={app_dir}/compose.web.yml
EnvironmentFile=-{app_dir}/.env
WorkingDirectory={app_dir}
ExecStart=/usr/bin/docker compose -p ${{PROJECT_NAME}} -f ${{COMPOSE_BASE}} -f ${{COMPOSE_OVERLAYS}} up -d web --remove-orphans
ExecStop=/usr/bin/docker compose -p ${{PROJECT_NAME}} -f ${{COMPOSE_BASE}} -f ${{COMPOSE_OVERLAYS}} stop web
Restart=no

[Install]
WantedBy=app-testapp.target
"#,
      app_dir = app_dir_str
    );
    assert_eq!(web_content, expected_web);

    // 4. Verify worker service
    let worker_path = systemd_dir.join("app-testapp-worker.service");
    assert!(worker_path.exists(), "Worker service should exist");
    let worker_content = fs::read_to_string(&worker_path)?;
    let expected_worker = format!(
      r#"[Unit]
Description=App testapp worker process
After=default.target app-testapp-acc.service
Wants=app-testapp-acc.service
PartOf=app-testapp.target

[Service]
Type=oneshot
RemainAfterExit=yes
ExecStartPre=/usr/bin/bash -lc 'for i in {{1..30}}; do docker version >/dev/null 2>&1 && exit 0; sleep 1; done; echo "Docker unavailable" >&2; exit 1'
Environment=PROJECT_NAME=testapp
Environment=COMPOSE_BASE={app_dir}/compose.yml
Environment=COMPOSE_OVERLAYS={app_dir}/compose.worker.yml
EnvironmentFile=-{app_dir}/.env
WorkingDirectory={app_dir}
ExecStart=/usr/bin/docker compose -p ${{PROJECT_NAME}} -f ${{COMPOSE_BASE}} -f ${{COMPOSE_OVERLAYS}} up -d worker --remove-orphans
ExecStartPost=/usr/bin/docker compose -p ${{PROJECT_NAME}} -f ${{COMPOSE_BASE}} -f ${{COMPOSE_OVERLAYS}} up -d --scale worker=${{WORKER_SCALE:-1}} worker
ExecStop=/usr/bin/docker compose -p ${{PROJECT_NAME}} -f ${{COMPOSE_BASE}} -f ${{COMPOSE_OVERLAYS}} stop worker
Restart=no

[Install]
WantedBy=app-testapp.target
"#,
      app_dir = app_dir_str
    );
    assert_eq!(worker_content, expected_worker);

    Ok(())
  }

  #[test]
  fn test_render_and_write_no_accessories() -> std::io::Result<()> {
    let temp_dir = TempDir::new()?;
    let systemd_dir = temp_dir.path().join("systemd");
    let app_dir = temp_dir.path().join("apps").join("simpleapp");
    let app_dir_str = app_dir.display().to_string();

    let spec = UnitsSpec {
      app_name: "simpleapp".to_string(),
      processes: vec!["web".to_string()],
      accessories: vec![],
      systemd_dir: systemd_dir.clone(),
      app_dir: app_dir.clone(),
      env_file: None,
    };

    let outcomes = render_and_write(&spec)?;

    // Should create 2 files: target and web service (no accessories service)
    assert_eq!(outcomes.len(), 2, "Should create 2 unit files");

    // Verify accessories service was NOT created
    let acc_path = systemd_dir.join("app-simpleapp-acc.service");
    assert!(!acc_path.exists(), "Accessories service should not exist");

    // Verify target file
    let target_path = systemd_dir.join("app-simpleapp.target");
    let target_content = fs::read_to_string(&target_path)?;
    let expected_target = "[Unit]
Description=App simpleapp stack
After=default.target
Wants=app-simpleapp-web.service

[Install]
WantedBy=default.target\n";
    assert_eq!(target_content, expected_target);

    // Verify web service
    let web_path = systemd_dir.join("app-simpleapp-web.service");
    let web_content = fs::read_to_string(&web_path)?;
    let expected_web = format!(
      r#"[Unit]
Description=App simpleapp web process
After=default.target
Wants=
PartOf=app-simpleapp.target

[Service]
Type=oneshot
RemainAfterExit=yes
ExecStartPre=/usr/bin/bash -lc 'for i in {{1..30}}; do docker version >/dev/null 2>&1 && exit 0; sleep 1; done; echo "Docker unavailable" >&2; exit 1'
Environment=PROJECT_NAME=simpleapp
Environment=COMPOSE_BASE={app_dir}/compose.yml
Environment=COMPOSE_OVERLAYS={app_dir}/compose.web.yml
WorkingDirectory={app_dir}
ExecStart=/usr/bin/docker compose -p ${{PROJECT_NAME}} -f ${{COMPOSE_BASE}} -f ${{COMPOSE_OVERLAYS}} up -d web --remove-orphans
ExecStop=/usr/bin/docker compose -p ${{PROJECT_NAME}} -f ${{COMPOSE_BASE}} -f ${{COMPOSE_OVERLAYS}} stop web
Restart=no

[Install]
WantedBy=app-simpleapp.target
"#,
      app_dir = app_dir_str
    );
    assert_eq!(web_content, expected_web);

    Ok(())
  }

  #[test]
  fn test_render_and_write_idempotent() -> std::io::Result<()> {
    let temp_dir = TempDir::new()?;
    let systemd_dir = temp_dir.path().join("systemd");
    let app_dir = temp_dir.path().join("apps").join("testapp");

    let spec = UnitsSpec {
      app_name: "testapp".to_string(),
      processes: vec!["web".to_string()],
      accessories: vec!["postgres".to_string()],
      systemd_dir: systemd_dir.clone(),
      app_dir: app_dir.clone(),
      env_file: Some(app_dir.join(".env")),
    };

    // First write
    let outcomes1 = render_and_write(&spec)?;
    assert_eq!(outcomes1.len(), 3);
    for outcome in &outcomes1 {
      match outcome {
        WriteOutcome::Created(_) => {}
        _ => panic!("Expected all files to be Created on first write"),
      }
    }

    // Second write (should be unchanged)
    let outcomes2 = render_and_write(&spec)?;
    assert_eq!(outcomes2.len(), 3);
    for outcome in &outcomes2 {
      match outcome {
        WriteOutcome::Unchanged(_) => {}
        _ => panic!("Expected all files to be Unchanged on second write"),
      }
    }

    Ok(())
  }

  #[test]
  fn test_render_and_write_update() -> std::io::Result<()> {
    let temp_dir = TempDir::new()?;
    let systemd_dir = temp_dir.path().join("systemd");
    let app_dir = temp_dir.path().join("apps").join("testapp");

    let spec1 = UnitsSpec {
      app_name: "testapp".to_string(),
      processes: vec!["web".to_string()],
      accessories: vec![],
      systemd_dir: systemd_dir.clone(),
      app_dir: app_dir.clone(),
      env_file: None,
    };

    // First write
    let outcomes1 = render_and_write(&spec1)?;
    assert_eq!(outcomes1.len(), 2);

    // Update spec to add accessories
    let spec2 = UnitsSpec {
      app_name: "testapp".to_string(),
      processes: vec!["web".to_string()],
      accessories: vec!["postgres".to_string()],
      systemd_dir: systemd_dir.clone(),
      app_dir: app_dir.clone(),
      env_file: None,
    };

    // Second write (should update target, web, and create acc)
    let outcomes2 = render_and_write(&spec2)?;
    assert_eq!(outcomes2.len(), 3);

    let mut has_created = false;
    let mut has_updated = false;

    for outcome in &outcomes2 {
      match outcome {
        WriteOutcome::Created(path) => {
          assert!(
            path.to_string_lossy().contains("acc.service"),
            "Only acc.service should be newly created"
          );
          has_created = true;
        }
        WriteOutcome::Updated(_) => {
          has_updated = true;
        }
        WriteOutcome::Unchanged(_) => {}
      }
    }

    assert!(has_created, "Should have created acc.service");
    assert!(has_updated, "Should have updated target or web service");

    Ok(())
  }
}
