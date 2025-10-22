use regex::Regex;
use std::fs;
use std::io::{self, Read};
use std::path::Path;

/// Finds process unit names by scanning app-<app>-*.service,
/// excluding the accessories unit (-acc.service).
pub fn discover_processes(systemd_dir: &Path, app: &str) -> std::io::Result<Vec<String>> {
  let mut procs = Vec::new();
  let pattern_prefix = format!("app-{}-", app);
  for entry in std::fs::read_dir(systemd_dir)? {
    let entry = entry?;
    let path = entry.path();
    if path.extension().and_then(|s| s.to_str()) != Some("service") {
      continue;
    }
    let fname = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
    if !fname.starts_with(&pattern_prefix) {
      continue;
    }
    if fname.ends_with("-acc.service") {
      continue;
    }
    // app-<app>-<proc>.service → extract <proc>
    if let Some(proc_part) = fname
      .strip_prefix(&pattern_prefix)
      .and_then(|s| s.strip_suffix(".service"))
    {
      procs.push(proc_part.to_string());
    }
  }
  procs.sort();
  procs.dedup();
  Ok(procs)
}

/// Discover existing accessories for an app.
///
/// Strategy:
///  1) If the systemd unit `app-<app>-acc.service` exists, parse Environment=COMPOSE_ACC
///     (colon-separated list of overlay paths) and extract the "<acc>" from `compose.<acc>.yml`.
///  2) Otherwise, fall back to scanning the app dir for `compose.*.yml`, excluding:
///     - `compose.yml` (base)
///     - any `compose.<proc>.yml` where <proc> is in `known_processes`
///
/// Returns a sorted, deduped list of accessory names.
pub fn discover_accessories(
  systemd_dir: &Path,
  app_dir: &Path,
  app: &str,
  known_processes: &[String],
) -> io::Result<Vec<String>> {
  // 1) Try to parse from unit file (single source of truth if present)
  let unit_path = systemd_dir.join(format!("app-{}-acc.service", app));
  if unit_path.exists() {
    let mut buf = String::new();
    fs::File::open(&unit_path)?.read_to_string(&mut buf)?;
    if let Some(accs) = parse_compose_acc_env(&buf) {
      let items = accs
        .into_iter()
        .filter_map(|p| extract_accessory_from_overlay_path(&p))
        .collect::<Vec<_>>();
      return Ok(sorted_dedup(items));
    }
    // If the unit exists but has no COMPOSE_ACC, fall through to filesystem scan.
  }

  // 2) Filesystem fallback: scan for compose.<name>.yml that aren't base or process overlays
  let mut accs = Vec::new();
  let base = app_dir.join("compose.yml");

  // quick lookup set for process overlay names to exclude
  let proc_set = known_processes
    .iter()
    .map(|s| s.as_str())
    .collect::<std::collections::HashSet<_>>();

  for entry in fs::read_dir(app_dir)? {
    let entry = entry?;
    let path = entry.path();
    if path == base {
      continue;
    }
    if path.extension().and_then(|s| s.to_str()) != Some("yml") {
      continue;
    }

    let fname = match path.file_name().and_then(|s| s.to_str()) {
      Some(s) => s,
      None => continue,
    };
    // match compose.<thing>.yml
    if let Some(stem) = fname
      .strip_prefix("compose.")
      .and_then(|s| s.strip_suffix(".yml"))
    {
      // skip process overlays
      if proc_set.contains(stem) {
        continue;
      }
      // treat it as accessory
      accs.push(stem.to_string());
    }
  }

  Ok(sorted_dedup(accs))
}

fn parse_compose_acc_env(unit_content: &str) -> Option<Vec<String>> {
  // Lines can be: Environment=COMPOSE_ACC=/srv/app/compose.redis.yml:/srv/app/compose.postgres.yml
  // Accept optional quotes and spaces.
  let re = Regex::new(r#"(?m)^Environment=COMPOSE_ACC=(?:"([^"]+)"|([^\r\n]+))$"#).unwrap();
  let caps = re.captures(unit_content)?;
  let raw = caps.get(1).or_else(|| caps.get(2))?.as_str();
  let parts = raw
    .split(':')
    .map(|s| s.trim().to_string())
    .filter(|s| !s.is_empty())
    .collect::<Vec<_>>();
  if parts.is_empty() {
    None
  } else {
    Some(parts)
  }
}

fn extract_accessory_from_overlay_path(path: &str) -> Option<String> {
  // …/compose.<acc>.yml → <acc>
  let fname = Path::new(path).file_name()?.to_str()?;
  let stem = fname.strip_prefix("compose.")?.strip_suffix(".yml")?;
  Some(stem.to_string())
}

fn sorted_dedup(mut v: Vec<String>) -> Vec<String> {
  v.sort();
  v.dedup();
  v
}
