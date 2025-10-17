use anyhow::{Context, Result};
use crate::log::debug;
use std::path::PathBuf;
use std::process::Stdio;
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
    debug(&format!("export_commit: repo_path={}, sha={}", repo_path, sha));

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
    debug(&format!("spawning git archive command: git --git-dir {} archive {}", repo_path, sha));

    let mut git_archive = Command::new("git")
        .arg("--git-dir")
        .arg(repo_path)
        .arg("archive")
        .arg(sha)
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .context(format!("Failed to spawn git archive (repo: {}, sha: {})", repo_path, sha))?;

    // Spawn tar extract process
    debug(&format!("spawning tar extract command: tar -xC {}", tmpdir.display()));

    let mut tar_extract = Command::new("tar")
        .arg("-xC")
        .arg(&tmpdir)
        .stdin(Stdio::piped())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .context(format!("Failed to spawn tar extract (target: {})", tmpdir.display()))?;

    debug("git archive and tar extract processes spawned successfully");

    // Pipe git archive stdout to tar stdin
    if let (Some(mut git_stdout), Some(mut tar_stdin)) =
        (git_archive.stdout.take(), tar_extract.stdin.take())
    {
        tokio::spawn(async move {
            tokio::io::copy(&mut git_stdout, &mut tar_stdin)
                .await
                .ok();
            tar_stdin.shutdown().await.ok();
        });
    }

    debug("waiting for git archive to complete...");

    // Wait for both processes to complete
    let git_status = git_archive
        .wait()
        .await
        .context("Failed to wait for git archive")?;

    debug(&format!("git archive completed with status: {}", git_status));

    debug("waiting for tar extract to complete...");

    let tar_status = tar_extract
        .wait()
        .await
        .context("Failed to wait for tar extract")?;

    debug(&format!("tar extract completed with status: {}", tar_status));

    if !git_status.success() {
        anyhow::bail!("git archive failed with status: {} (repo: {}, sha: {})", git_status, repo_path, sha);
    }

    if !tar_status.success() {
        anyhow::bail!("tar extract failed with status: {} (target: {})", tar_status, tmpdir.display());
    }

    debug(&format!("successfully exported commit {} to {}", sha, tmpdir.display()));

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
}
