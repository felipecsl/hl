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
            app_dir
                .join(format!("compose.{a}.yml"))
                .display()
                .to_string()
        })
        .collect::<Vec<_>>()
        .join(":");

    let mut body = String::new();
    writeln!(
        &mut body,
        r#"[Unit]
Description=App {app} accessories (Redis/Postgres/etc.)
After=default.target

[Service]
Type=oneshot
RemainAfterExit=yes
ExecStartPre=/usr/bin/bash -lc 'for i in {{1..30}}; do docker version >/dev/null 2>&1 && exit 0; sleep 1; done; echo "Docker unavailable" >&2; exit 1'
Environment=PROJECT_NAME={project}
Environment=COMPOSE_BASE={base}
Environment=COMPOSE_ACC={acc_files}
WorkingDirectory={app_dir}
ExecStart=/usr/bin/docker compose -p ${{PROJECT_NAME}} -f ${{COMPOSE_BASE}} -f ${{COMPOSE_ACC}} up -d
ExecStop=/usr/bin/docker compose -p ${{PROJECT_NAME}} -f ${{COMPOSE_BASE}} -f ${{COMPOSE_ACC}} stop
Restart=no

[Install]
WantedBy=default.target"#,
        app = app,
        project = project,
        base = base.display(),
        acc_files = acc_files,
        app_dir = app_dir.display()
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
    writeln!(&mut unit, "ExecStartPre=/usr/bin/bash -lc 'for i in {{1..30}}; do docker version >/dev/null 2>&1 && exit 0; sleep 1; done; echo \"Docker unavailable\" >&2; exit 1'").unwrap();
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
        "ExecStart=/usr/bin/docker compose -p ${{PROJECT_NAME}} -f ${{COMPOSE_BASE}} -f ${{COMPOSE_OVERLAYS}} up -d {svc}",
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
    // systemd is forgiving here; we’ll just avoid raw newlines and quotes.
    s.replace('"', r#"\""#).replace('\n', " ")
}
