use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::Path;

/// Parse a Procfile and return a map of process names to commands
///
/// Procfile format (similar to Heroku):
/// ```text
/// web: bundle exec rails server -p $PORT
/// worker: bundle exec sidekiq
/// release: bundle exec rake db:migrate
/// ```
///
/// # Arguments
/// * `procfile_path` - Path to the Procfile
///
/// # Returns
/// HashMap mapping process names (e.g., "web", "worker") to their commands
pub async fn parse_procfile(procfile_path: &Path) -> Result<HashMap<String, String>> {
    let content = tokio::fs::read_to_string(procfile_path)
        .await
        .context(format!("Failed to read Procfile at {}", procfile_path.display()))?;

    let mut processes = HashMap::new();

    for (line_num, line) in content.lines().enumerate() {
        let line = line.trim();

        // Skip empty lines and comments
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        // Parse "process_name: command" format
        if let Some((process_name, command)) = line.split_once(':') {
            let process_name = process_name.trim();
            let command = command.trim();

            if process_name.is_empty() {
                anyhow::bail!(
                    "Invalid Procfile format at line {}: empty process name",
                    line_num + 1
                );
            }

            if command.is_empty() {
                anyhow::bail!(
                    "Invalid Procfile format at line {}: empty command for process '{}'",
                    line_num + 1,
                    process_name
                );
            }

            // Check for duplicate process names
            if processes.contains_key(process_name) {
                anyhow::bail!(
                    "Duplicate process name '{}' at line {}",
                    process_name,
                    line_num + 1
                );
            }

            processes.insert(process_name.to_string(), command.to_string());
        } else {
            anyhow::bail!(
                "Invalid Procfile format at line {}: expected 'process_name: command', got '{}'",
                line_num + 1,
                line
            );
        }
    }

    Ok(processes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[tokio::test]
    async fn test_parse_procfile_basic() {
        let mut tmpfile = NamedTempFile::new().unwrap();
        writeln!(tmpfile, "web: bundle exec rails server -p $PORT").unwrap();
        writeln!(tmpfile, "worker: bundle exec sidekiq").unwrap();

        let processes = parse_procfile(tmpfile.path()).await.unwrap();

        assert_eq!(processes.len(), 2);
        assert_eq!(
            processes.get("web"),
            Some(&"bundle exec rails server -p $PORT".to_string())
        );
        assert_eq!(
            processes.get("worker"),
            Some(&"bundle exec sidekiq".to_string())
        );
    }

    #[tokio::test]
    async fn test_parse_procfile_with_comments() {
        let mut tmpfile = NamedTempFile::new().unwrap();
        writeln!(tmpfile, "# This is a comment").unwrap();
        writeln!(tmpfile, "web: npm start").unwrap();
        writeln!(tmpfile, "").unwrap();
        writeln!(tmpfile, "# Another comment").unwrap();
        writeln!(tmpfile, "worker: node worker.js").unwrap();

        let processes = parse_procfile(tmpfile.path()).await.unwrap();

        assert_eq!(processes.len(), 2);
        assert_eq!(processes.get("web"), Some(&"npm start".to_string()));
        assert_eq!(
            processes.get("worker"),
            Some(&"node worker.js".to_string())
        );
    }

    #[tokio::test]
    async fn test_parse_procfile_with_whitespace() {
        let mut tmpfile = NamedTempFile::new().unwrap();
        writeln!(tmpfile, "  web  :   bundle exec rails server   ").unwrap();
        writeln!(tmpfile, "worker:bundle exec sidekiq").unwrap();

        let processes = parse_procfile(tmpfile.path()).await.unwrap();

        assert_eq!(processes.len(), 2);
        assert_eq!(
            processes.get("web"),
            Some(&"bundle exec rails server".to_string())
        );
        assert_eq!(
            processes.get("worker"),
            Some(&"bundle exec sidekiq".to_string())
        );
    }

    #[tokio::test]
    async fn test_parse_procfile_empty() {
        let tmpfile = NamedTempFile::new().unwrap();

        let processes = parse_procfile(tmpfile.path()).await.unwrap();

        assert_eq!(processes.len(), 0);
    }

    #[tokio::test]
    async fn test_parse_procfile_duplicate_process_name() {
        let mut tmpfile = NamedTempFile::new().unwrap();
        writeln!(tmpfile, "web: npm start").unwrap();
        writeln!(tmpfile, "web: node server.js").unwrap();

        let result = parse_procfile(tmpfile.path()).await;

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Duplicate process name 'web'"));
    }

    #[tokio::test]
    async fn test_parse_procfile_invalid_format() {
        let mut tmpfile = NamedTempFile::new().unwrap();
        writeln!(tmpfile, "web npm start").unwrap();

        let result = parse_procfile(tmpfile.path()).await;

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Invalid Procfile format"));
    }

    #[tokio::test]
    async fn test_parse_procfile_empty_command() {
        let mut tmpfile = NamedTempFile::new().unwrap();
        writeln!(tmpfile, "web:").unwrap();

        let result = parse_procfile(tmpfile.path()).await;

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("empty command for process 'web'"));
    }

    #[tokio::test]
    async fn test_parse_procfile_empty_process_name() {
        let mut tmpfile = NamedTempFile::new().unwrap();
        writeln!(tmpfile, ": npm start").unwrap();

        let result = parse_procfile(tmpfile.path()).await;

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("empty process name"));
    }
}
