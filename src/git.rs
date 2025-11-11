use crate::log::debug;
use anyhow::{Context, Result};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

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
  {}/.local/bin/hl deploy --app {} --sha "$newrev" --branch "$branch"
done
"#,
    home_dir, app_name
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
  {}/.hl/bin/hl deploy --app {} --sha "$newrev" --branch "$branch"
done
"#,
      home_dir, app_name
    );
    assert_eq!(hook_contents, expected_hook);

    // Cleanup
    tokio::fs::remove_dir_all(&git_dir).await.ok();
  }
}
