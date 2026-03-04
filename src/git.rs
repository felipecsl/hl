use crate::{config::hl_root, log::debug};
use anyhow::{Context, Result};
use regex::Regex;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::OnceLock;
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

/// Compiled regex for parsing app names from hl git remote URLs.
static APP_NAME_RE: OnceLock<Regex> = OnceLock::new();

/// Compiled regex for validating app names read from the `HL_APP` env var.
static VALID_NAME_RE: OnceLock<Regex> = OnceLock::new();

/// Parse an app name from a git remote URL matching the hl convention.
/// Expects URLs containing `/hl/git/<app>.git`.
pub fn parse_app_name_from_remote_url(url: &str) -> Option<String> {
  let re = APP_NAME_RE
    .get_or_init(|| Regex::new(r"/hl/git/([^/]+)\.git\b").expect("APP_NAME_RE is a valid regex"));
  re.captures(url).map(|c| c[1].to_string())
}

/// Infer the app name from the `HL_APP` env var or from git remotes in the current directory.
pub async fn infer_app_name() -> Result<String> {
  let cwd = std::env::current_dir().context("Failed to read current working directory")?;

  // Check HL_APP env var first
  if let Ok(app) = std::env::var("HL_APP") {
    let app = app.trim().to_string();
    if !app.is_empty() {
      let valid_name_re = VALID_NAME_RE
        .get_or_init(|| Regex::new(r"^[A-Za-z0-9_-]+$").expect("VALID_NAME_RE is a valid regex"));
      if !valid_name_re.is_match(&app) {
        anyhow::bail!(
          "Invalid HL_APP value {:?}. App names may only contain letters, digits, '-' and '_'",
          app
        );
      }
      return Ok(app);
    }
  }

  // If running inside an hl app directory, infer from path:
  // /home/<user>/hl/apps/<app>[/...]
  if let Some(app) = infer_app_name_from_hl_app_dir() {
    return Ok(app);
  }

  // Try reading local .git/config directly first (works even if git subprocess
  // is affected by environment overrides).
  if let Some(mut app_names) = infer_app_names_from_local_git_config(&cwd) {
    app_names.sort();
    app_names.dedup();
    match app_names.len() {
      0 => {}
      1 => return Ok(app_names.into_iter().next().unwrap()),
      _ => {
        anyhow::bail!(
          "Multiple hl apps found in git remotes: {}. Set HL_APP to pick one.",
          app_names.join(", ")
        );
      }
    }
  }

  // Run `git remote -v` and parse output
  let output = Command::new("git")
    .arg("-C")
    .arg(&cwd)
    .args(["remote", "-v"])
    // Ignore caller-provided repository overrides for deterministic inference.
    .env_remove("GIT_DIR")
    .env_remove("GIT_WORK_TREE")
    .output()
    .await;

  let output = match output {
    Ok(o) => o,
    Err(_) => {
      anyhow::bail!(
        "Could not infer app name. Set HL_APP, run from /home/<user>/hl/apps/<app>, or run from a git project directory with an hl remote."
      );
    }
  };

  if !output.status.success() {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if stderr.is_empty() {
      anyhow::bail!(
        "Could not infer app name. Set HL_APP, run from /home/<user>/hl/apps/<app>, or run from a git project directory with an hl remote."
      );
    }
    anyhow::bail!(
      "Could not infer app name from git remotes: {}. Set HL_APP, run from /home/<user>/hl/apps/<app>, or run from a git project directory with an hl remote.",
      stderr
    );
  }

  let stdout = String::from_utf8_lossy(&output.stdout);
  let mut app_names: Vec<String> = stdout
    .lines()
    .filter_map(|line| {
      // Each line is: <name>\t<url> (fetch|push)
      let url = line.split_whitespace().nth(1)?;
      parse_app_name_from_remote_url(url)
    })
    .collect();

  app_names.sort();
  app_names.dedup();

  match app_names.len() {
    0 => {
      anyhow::bail!(
        "No hl remote found. Add a remote like:\n  git remote add production ssh://user@host/home/user/hl/git/<app>.git"
      );
    }
    1 => Ok(app_names.into_iter().next().unwrap()),
    _ => {
      anyhow::bail!(
        "Multiple hl apps found in git remotes: {}. Set HL_APP to pick one.",
        app_names.join(", ")
      );
    }
  }
}

fn infer_app_name_from_hl_app_dir() -> Option<String> {
  let cwd = std::env::current_dir().ok()?;
  let root = hl_root();

  for dir in cwd.ancestors() {
    if dir.parent() == Some(root.as_path()) {
      let app = dir.file_name()?.to_string_lossy().trim().to_string();
      if !app.is_empty() {
        return Some(app);
      }
    }
  }

  None
}

fn infer_app_names_from_local_git_config(start: &Path) -> Option<Vec<String>> {
  let repo_root = start
    .ancestors()
    .find(|dir| dir.join(".git").exists())
    .map(Path::to_path_buf)?;

  let git_entry = repo_root.join(".git");
  let git_dir = if git_entry.is_dir() {
    git_entry
  } else if git_entry.is_file() {
    let content = std::fs::read_to_string(&git_entry).ok()?;
    let rel_or_abs = content
      .lines()
      .find_map(|line| line.strip_prefix("gitdir:"))
      .map(str::trim)?;
    let path = PathBuf::from(rel_or_abs);
    if path.is_absolute() {
      path
    } else {
      repo_root.join(path)
    }
  } else {
    return None;
  };

  let config = std::fs::read_to_string(git_dir.join("config")).ok()?;
  let mut in_remote = false;
  let mut app_names = Vec::new();

  for raw_line in config.lines() {
    let line = raw_line.trim();
    if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
      continue;
    }

    if line.starts_with('[') && line.ends_with(']') {
      in_remote = line.starts_with("[remote ");
      continue;
    }

    if !in_remote {
      continue;
    }

    let Some((key, value)) = line.split_once('=') else {
      continue;
    };

    if key.trim() == "url" {
      if let Some(app) = parse_app_name_from_remote_url(value.trim()) {
        app_names.push(app);
      }
    }
  }

  Some(app_names)
}

/// Export a git commit to a temporary directory
///
/// This uses `git archive` to stream the commit contents as a tar,
/// then pipes it to `tar -x` to extract into a temporary directory.
///
/// # Arguments
/// * `repo_path` - Path to the git repository (can be a .git directory)
/// * `sha` - Git commit SHA to export
///
/// # Returns
/// Path to the temporary directory containing the exported commit
pub async fn export_commit(repo_path: &str, sha: &str) -> Result<PathBuf> {
  debug(&format!(
    "export_commit: repo_path={}, sha={}",
    repo_path, sha
  ));

  // Check if the git repository exists
  let repo_path_buf = PathBuf::from(repo_path);
  if !repo_path_buf.exists() {
    anyhow::bail!("Git repository not found at: {}", repo_path);
  }
  debug(&format!("git repository exists at: {}", repo_path));

  // Create unique temp directory
  let tmpdir = tokio::fs::canonicalize(std::env::temp_dir())
    .await
    .context("Failed to canonicalize temp dir")?;

  debug(&format!("temp dir base: {}", tmpdir.display()));

  let tmpdir = create_temp_dir(&tmpdir, sha).await?;

  debug(&format!("created temp dir: {}", tmpdir.display()));

  // Spawn git archive process
  debug(&format!(
    "spawning git archive command: git --git-dir {} archive {}",
    repo_path, sha
  ));

  let mut git_archive = Command::new("git")
    .arg("--git-dir")
    .arg(repo_path)
    .arg("archive")
    .arg(sha)
    .stdout(Stdio::piped())
    .stderr(Stdio::inherit())
    .spawn()
    .context(format!(
      "Failed to spawn git archive (repo: {}, sha: {})",
      repo_path, sha
    ))?;

  // Spawn tar extract process
  debug(&format!(
    "spawning tar extract command: tar -xC {}",
    tmpdir.display()
  ));

  let mut tar_extract = Command::new("tar")
    .arg("-xC")
    .arg(&tmpdir)
    .stdin(Stdio::piped())
    .stdout(Stdio::inherit())
    .stderr(Stdio::inherit())
    .spawn()
    .context(format!(
      "Failed to spawn tar extract (target: {})",
      tmpdir.display()
    ))?;

  debug("git archive and tar extract processes spawned successfully");

  // Pipe git archive stdout to tar stdin
  if let (Some(mut git_stdout), Some(mut tar_stdin)) =
    (git_archive.stdout.take(), tar_extract.stdin.take())
  {
    tokio::spawn(async move {
      tokio::io::copy(&mut git_stdout, &mut tar_stdin).await.ok();
      tar_stdin.shutdown().await.ok();
    });
  }

  debug("waiting for git archive to complete...");

  // Wait for both processes to complete
  let git_status = git_archive
    .wait()
    .await
    .context("Failed to wait for git archive")?;

  debug(&format!(
    "git archive completed with status: {}",
    git_status
  ));

  debug("waiting for tar extract to complete...");

  let tar_status = tar_extract
    .wait()
    .await
    .context("Failed to wait for tar extract")?;

  debug(&format!(
    "tar extract completed with status: {}",
    tar_status
  ));

  if !git_status.success() {
    anyhow::bail!(
      "git archive failed with status: {} (repo: {}, sha: {})",
      git_status,
      repo_path,
      sha
    );
  }

  if !tar_status.success() {
    anyhow::bail!(
      "tar extract failed with status: {} (target: {})",
      tar_status,
      tmpdir.display()
    );
  }

  debug(&format!(
    "successfully exported commit {} to {}",
    sha,
    tmpdir.display()
  ));

  Ok(tmpdir)
}

/// Create a unique temporary directory with the given prefix
async fn create_temp_dir(base: &std::path::Path, sha: &str) -> Result<PathBuf> {
  let prefix = format!("hl-{}-", &sha[..7.min(sha.len())]);

  // Try to create temp directory with incrementing suffix
  for i in 0..100 {
    let suffix = if i == 0 {
      String::new()
    } else {
      format!("{}", i)
    };

    let tmpdir = base.join(format!("{}{}", prefix, suffix));

    match tokio::fs::create_dir(&tmpdir).await {
      Ok(_) => return Ok(tmpdir),
      Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
      Err(e) => return Err(e).context("Failed to create temp directory"),
    }
  }

  anyhow::bail!("Failed to create unique temp directory after 100 attempts")
}

/// Generate the SSH URI for a git repository
/// Given the git directory path, constructs an SSH URI
/// using the current user's username and the system's hostname.
/// # Arguments
/// * `git_dir` - Path to the git repository
/// # Returns
/// SSH URI string in the format: ssh://username@hostname/git_dir
pub fn repo_remote_uri(git_dir: &str) -> String {
  let username = std::env::var("USER").unwrap_or_else(|_| "user".to_string());
  let hostname = std::process::Command::new("hostname")
    .output()
    .ok()
    .and_then(|output| String::from_utf8(output.stdout).ok())
    .map(|s| s.trim().to_string())
    .unwrap_or_else(|| "hostname".to_string());
  format!("ssh://{}@{}{}", username, hostname, git_dir)
}

/// Initialize a bare git repository with a post-receive hook
///
/// Creates a bare git repository at the specified path and installs a post-receive
/// hook that triggers `hl deploy` when commits are pushed.
///
/// # Arguments
/// * `git_dir` - Path where the bare repository should be created
/// * `app_name` - Name of the application (used in hook script)
/// * `home_dir` - Home directory path (used to locate hl binary)
///
/// # Returns
/// Ok(()) on success, or an error if repository creation or hook installation fails
pub async fn init_bare_repo(git_dir: &Path, app_name: &str, home_dir: &str) -> Result<()> {
  debug(&format!(
    "initializing bare git repository at: {}",
    git_dir.display()
  ));

  // Create parent directory if needed
  if let Some(parent) = git_dir.parent() {
    fs::create_dir_all(parent)
      .await
      .context("Failed to create parent directory for git repo")?;
  }

  // Initialize bare git repository
  let status = Command::new("git")
    .arg("init")
    .arg("--bare")
    .arg(git_dir)
    .status()
    .await
    .context("Failed to run git init")?;

  if !status.success() {
    anyhow::bail!("git init failed with status: {}", status);
  }

  debug("bare git repository initialized successfully");

  // Create post-receive hook
  let hooks_dir = git_dir.join("hooks");
  let hook_path = hooks_dir.join("post-receive");

  let hook_content = format!(
    r#"#!/usr/bin/env bash
set -euo pipefail
while read -r oldrev newrev refname; do
  case "$refname" in refs/heads/*) branch="${{refname#refs/heads/}}";;
    *) continue;;
  esac
  HL_APP={} {}/.local/bin/hl deploy --sha "$newrev" --branch "$branch"
done
"#,
    app_name, home_dir
  );

  fs::write(&hook_path, hook_content)
    .await
    .context("Failed to write post-receive hook")?;

  debug("post-receive hook written");

  // Make hook executable
  let mut perms = fs::metadata(&hook_path)
    .await
    .context("Failed to read hook metadata")?
    .permissions();
  perms.set_mode(0o755);
  fs::set_permissions(&hook_path, perms)
    .await
    .context("Failed to set hook permissions")?;

  debug("post-receive hook made executable");

  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;
  use serial_test::serial;
  use tempfile::TempDir;

  #[tokio::test]
  async fn test_create_temp_dir() {
    let base = std::env::temp_dir();
    let sha = "abc1234567890";

    let tmpdir = create_temp_dir(&base, sha).await.unwrap();
    assert!(tmpdir.exists());
    assert!(tmpdir.to_string_lossy().contains("hl-abc1234-"));

    // Cleanup
    tokio::fs::remove_dir(&tmpdir).await.ok();
  }

  #[tokio::test]
  async fn test_init_bare_repo() {
    use std::os::unix::fs::PermissionsExt;

    let base = std::env::temp_dir();
    let git_dir = base.join(format!("test-bare-repo-{}", rand::random::<u32>()));
    let app_name = "testapp";
    let home_dir = "/home/testuser";

    init_bare_repo(&git_dir, app_name, home_dir).await.unwrap();

    // Assert that the git directory was created
    assert!(git_dir.exists());

    // Assert that it's a valid git repository (has refs, objects, HEAD)
    assert!(git_dir.join("refs").exists());
    assert!(git_dir.join("objects").exists());
    assert!(git_dir.join("HEAD").exists());

    // Assert that the post-receive hook exists and is executable
    let hook_path = git_dir.join("hooks").join("post-receive");
    assert!(hook_path.exists());

    let hook_metadata = tokio::fs::metadata(&hook_path).await.unwrap();
    let permissions = hook_metadata.permissions();
    assert_eq!(permissions.mode() & 0o111, 0o111); // Check executable bits

    let hook_contents = tokio::fs::read_to_string(&hook_path).await.unwrap();
    let expected_hook = format!(
      r#"#!/usr/bin/env bash
set -euo pipefail
while read -r oldrev newrev refname; do
  case "$refname" in refs/heads/*) branch="${{refname#refs/heads/}}";;
    *) continue;;
  esac
  HL_APP={} {}/.local/bin/hl deploy --sha "$newrev" --branch "$branch"
done
"#,
      app_name, home_dir
    );
    assert_eq!(hook_contents, expected_hook);

    // Cleanup
    tokio::fs::remove_dir_all(&git_dir).await.ok();
  }

  #[test]
  fn test_parse_app_name_from_remote_url_ssh() {
    let url = "ssh://deploy@myhost/home/deploy/hl/git/myapp.git";
    assert_eq!(
      parse_app_name_from_remote_url(url),
      Some("myapp".to_string())
    );
  }

  #[test]
  fn test_parse_app_name_from_remote_url_local_path() {
    let url = "/home/user/hl/git/webapp.git";
    assert_eq!(
      parse_app_name_from_remote_url(url),
      Some("webapp".to_string())
    );
  }

  #[test]
  fn test_parse_app_name_from_remote_url_no_match() {
    assert_eq!(
      parse_app_name_from_remote_url("git@github.com:user/repo.git"),
      None
    );
    assert_eq!(
      parse_app_name_from_remote_url("https://github.com/user/repo"),
      None
    );
  }

  #[test]
  fn test_parse_app_name_from_remote_url_with_dashes() {
    let url = "ssh://u@h/home/u/hl/git/my-cool-app.git";
    assert_eq!(
      parse_app_name_from_remote_url(url),
      Some("my-cool-app".to_string())
    );
  }

  #[tokio::test]
  #[serial]
  async fn test_infer_app_name_from_env() {
    std::env::set_var("HL_APP", "envapp");
    let result = infer_app_name().await;
    std::env::remove_var("HL_APP");
    assert_eq!(result.unwrap(), "envapp");
  }

  #[tokio::test]
  #[serial]
  async fn test_infer_app_name_empty_env_falls_through() {
    std::env::set_var("HL_APP", "");
    let result = infer_app_name().await;
    std::env::remove_var("HL_APP");
    // Should not return Ok("") — it should fall through and either find remotes or error
    assert!(result.is_err() || result.unwrap() != "");
  }

  #[tokio::test]
  #[serial]
  async fn test_infer_app_name_from_hl_app_dir() -> Result<()> {
    let tmp = TempDir::new()?;
    let hl_root = tmp.path().join("apps");
    let app_dir = hl_root.join("myapp");
    tokio::fs::create_dir_all(app_dir.join("nested")).await?;

    let original_cwd = std::env::current_dir()?;
    std::env::set_var("HL_ROOT_OVERRIDE", &hl_root);
    std::env::set_current_dir(app_dir.join("nested"))?;

    let result = infer_app_name().await;

    std::env::set_current_dir(original_cwd)?;
    std::env::remove_var("HL_ROOT_OVERRIDE");

    assert_eq!(result?, "myapp");
    Ok(())
  }

  #[tokio::test]
  #[serial]
  async fn test_infer_app_name_from_git_config_file() -> Result<()> {
    let tmp = TempDir::new()?;
    let repo_dir = tmp.path().join("repo");
    let git_dir = repo_dir.join(".git");
    tokio::fs::create_dir_all(&git_dir).await?;
    tokio::fs::write(
      git_dir.join("config"),
      r#"[remote "origin"]
  url = git@github.com:user/repo.git
[remote "production"]
  url = ssh://deploy@host/home/deploy/hl/git/myrepo.git
"#,
    )
    .await?;

    let original_cwd = std::env::current_dir()?;
    std::env::set_current_dir(&repo_dir)?;
    let result = infer_app_name().await;
    std::env::set_current_dir(original_cwd)?;

    assert_eq!(result?, "myrepo");
    Ok(())
  }
}
