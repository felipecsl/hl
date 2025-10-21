use crate::config::{app_dir, env_file, HLConfig};
use crate::log::debug;
use crate::systemd::restart_app_target;
use anyhow::Result;
use std::path::Path;
use std::process::Stdio;
use tokio::fs;
use tokio::process::Command;

pub struct BuildPushOptions {
    pub context: String,
    pub dockerfile: Option<String>,
    pub tags: Vec<String>,
    pub platforms: Option<String>,
}

pub async fn build_and_push(opts: BuildPushOptions) -> Result<()> {
    debug(&format!(
        "build_and_push: context={}, dockerfile={:?}",
        opts.context, opts.dockerfile
    ));

    // Verify context directory exists
    let context_path = std::path::Path::new(&opts.context);
    if !context_path.exists() {
        anyhow::bail!("Build context directory not found: {}", opts.context);
    }

    // Verify dockerfile exists if specified
    if let Some(ref dockerfile) = opts.dockerfile {
        let dockerfile_path = std::path::Path::new(dockerfile);
        if !dockerfile_path.exists() {
            anyhow::bail!("Dockerfile not found: {}", dockerfile);
        }
    }

    let mut args = vec!["buildx", "build", "--push"];

    if let Some(platforms) = &opts.platforms {
        args.push("--platform");
        args.push(platforms);
    }

    for tag in &opts.tags {
        args.push("-t");
        args.push(tag);
    }

    if let Some(dockerfile) = &opts.dockerfile {
        args.push("--file");
        args.push(dockerfile);
    }

    args.push(&opts.context);

    debug(&format!(
        "executing docker command: docker {}",
        args.join(" ")
    ));

    let status = Command::new("docker")
        .args(&args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .await?;

    if !status.success() {
        anyhow::bail!("docker build failed with status: {}", status);
    }

    debug("docker build completed successfully");

    Ok(())
}

pub async fn retag_latest(image: &str, from_tag: &str) -> Result<()> {
    // Pull the source image
    let status = Command::new("docker")
        .args(["pull", from_tag])
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .await?;

    if !status.success() {
        anyhow::bail!("docker pull failed");
    }

    // Tag it as latest
    let latest = format!("{}:latest", image);
    let status = Command::new("docker")
        .args(["tag", from_tag, &latest])
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .await?;

    if !status.success() {
        anyhow::bail!("docker tag failed");
    }

    // Push latest
    let status = Command::new("docker")
        .args(["push", &latest])
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .await?;

    if !status.success() {
        anyhow::bail!("docker push failed");
    }

    Ok(())
}

pub async fn restart_compose(
    cfg: &HLConfig,
    processes: &[String],
    accessories: &[String],
) -> Result<()> {
    let dir = app_dir(&cfg.app);

    debug(&format!("restart_compose: app_dir={}", dir.display()));

    if !dir.exists() {
        anyhow::bail!("App directory not found: {}", dir.display());
    }

    let mut args = vec!["compose".to_string()];
    args.push("-f".into());
    args.push("compose.yml".into());
    for name in processes.iter().chain(accessories.iter()) {
        args.push("-f".into());
        args.push(format!("compose.{name}.yml"));
    }
    args.push("pull".into());

    debug("pulling latest images with docker compose");

    let status = Command::new("docker")
        .args(&args)
        .current_dir(&dir)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .await?;

    if !status.success() {
        anyhow::bail!("docker compose pull failed with status: {}", status);
    }

    restart_app_target(&cfg.app).await?;

    Ok(())
}

/// Build the docker run command arguments for migrations
fn build_migration_args(cfg: &HLConfig, image_tag: &str, env_path: &str) -> Vec<String> {
    let mut args = vec!["run".to_string(), "--rm".to_string()];

    // Add env file
    args.push("--env-file".to_string());
    args.push(env_path.to_string());

    // Add environment variables
    for (k, v) in &cfg.migrations.env {
        args.push("-e".to_string());
        args.push(format!("{}={}", k, v));
    }

    // Add network
    args.push("--network".to_string());
    args.push(cfg.network.clone());

    // Add image
    args.push(image_tag.to_string());

    // Add command
    for cmd_part in &cfg.migrations.command {
        args.push(cmd_part.clone());
    }

    args
}

pub async fn run_migrations(cfg: &HLConfig, image_tag: &str) -> Result<()> {
    let dir = app_dir(&cfg.app);
    let env_path = env_file(&cfg.app);
    let env_path_str = env_path.to_string_lossy().to_string();

    debug(&format!(
        "run_migrations: app_dir={}, env_file={}, image={}",
        dir.display(),
        env_path.display(),
        image_tag
    ));

    if !dir.exists() {
        anyhow::bail!("App directory not found: {}", dir.display());
    }

    if !env_path.exists() {
        debug(&format!(
            "Warning: .env file not found at: {}",
            env_path.display()
        ));
    }

    let args = build_migration_args(cfg, image_tag, &env_path_str);

    debug(&format!(
        "executing migrations with docker command: docker {}",
        args.join(" ")
    ));

    let status = Command::new("docker")
        .args(&args)
        .current_dir(&dir)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .await?;

    if !status.success() {
        anyhow::bail!("migrations failed with status: {}", status);
    }

    debug("migrations completed successfully");

    Ok(())
}

pub struct ImageTags {
    pub sha: String,
    pub branch_sha: String,
    pub latest: String,
}

pub fn tag_for(cfg: &HLConfig, sha: &str, branch: &str) -> ImageTags {
    let short = &sha[..7.min(sha.len())];
    ImageTags {
        sha: format!("{}:{}", cfg.image, short),
        branch_sha: format!("{}:{}-{}", cfg.image, branch, short),
        latest: format!("{}:latest", cfg.image),
    }
}

/// Generate the base compose.yml file content for an application
pub async fn write_base_compose_file(dir: &Path, image: &str, network: &str) -> Result<()> {
    let compose = format!(
        r#"
services:
  base:
    image: {image}:latest
    restart: unless-stopped
    env_file: [.env]
    networks: [{network}]
    profiles: ["_template"]
networks:
  {network}:
    external: true
    name: {network}
"#,
        image = image,
        network = network
    );
    let compose_path = dir.join("compose.yml");
    fs::write(&compose_path, compose).await?;
    Ok(())
}

/// Generate process-specific compose files from a Procfile
///
/// For each process in the map, creates a `compose.{process}.yml` file
/// with the process name and command. If processes is None, creates a
/// default `compose.web.yml` with no command override.
///
/// # Arguments
/// * `dir` - Directory where compose files should be written
/// * `processes` - Optional map of process names to commands from Procfile
/// * `app` - Application name for Traefik labels
/// * `resolver` - Traefik certificate resolver name
pub async fn write_process_compose_files(
    dir: &Path,
    processes: Option<&std::collections::HashMap<String, String>>,
    app: &str,
    resolver: &str,
) -> Result<()> {
    if let Some(procs) = processes {
        // Generate a compose file for each process
        for (process_name, command) in procs {
            let compose_content =
                generate_process_compose(process_name, Some(command), app, resolver);
            let compose_path = dir.join(format!("compose.{}.yml", process_name));
            fs::write(&compose_path, compose_content).await?;
            debug(&format!(
                "wrote process compose file: {}",
                compose_path.display()
            ));
        }
    } else {
        // No Procfile, create default web process (will use default Dockerfile CMD)
        let compose_content = generate_process_compose("web", None, app, resolver);
        let compose_path = dir.join("compose.web.yml");
        fs::write(&compose_path, compose_content).await?;
        debug(&format!(
            "wrote default web compose file: {}",
            compose_path.display()
        ));
    }
    Ok(())
}

/// Generate the YAML content for a process-specific compose file
fn generate_process_compose(
    process_name: &str,
    command: Option<&String>,
    app: &str,
    resolver: &str,
) -> String {
    let mut service_def = format!(
        r#"
services:
  {process_name}:
    extends:
      file: ./compose.yml
      service: base
"#,
        process_name = process_name
    );

    // Add Traefik labels only for web process
    if process_name == "web" {
        service_def.push_str(&format!(
            r#"
    container_name: {app}
    labels:
      traefik.enable: true
      traefik.http.routers.{app}.rule: Host(`${{DOMAIN}}`)
      traefik.http.routers.{app}.entrypoints: websecure
      traefik.http.routers.{app}.tls.certresolver: {resolver}
      traefik.http.services.{app}.loadbalancer.server.port: ${{SERVICE_PORT}}"#,
            app = app,
            resolver = resolver
        ));
    }

    // Add command override if provided
    if let Some(cmd) = command {
        // Parse command string into individual arguments
        let args = match shell_words::split(cmd) {
            Ok(parts) => parts,
            Err(_) => vec![cmd.to_string()],
        };
        let args_yaml = args
            .iter()
            .map(|arg| format!("\"{}\"", arg))
            .collect::<Vec<_>>()
            .join(",");

        service_def.push_str(&format!(
            r#"
    command: [{}]"#,
            args_yaml
        ));
    }

    // Close the YAML with a newline
    service_def.push('\n');

    service_def
}

/// Wait for postgres to be ready by executing pg_isready inside a container.
/// Uses docker compose exec to probe the postgres service.
pub async fn wait_for_postgres_ready(app: &str) -> Result<()> {
    let dir = app_dir(app);

    if !dir.exists() {
        anyhow::bail!("App directory not found: {}", dir.display());
    }

    let mut compose_files = vec!["-f".to_string(), "compose.yml".to_string()];
    compose_files.push("-f".to_string());
    compose_files.push("compose.postgres.yml".to_string());

    let project_name = format!("{}-acc", app);
    debug(&format!(
        "waiting for postgres to be ready (project: {}, timeout: 60s)",
        project_name
    ));

    // Build the probe command: pg_isready with retry loop
    let probe_script = "for i in $(seq 1 60); do pg_isready -h 127.0.0.1 -p ${POSTGRES_PORT:-5432} && exit 0; sleep 1; done; exit 1";

    let mut args = vec!["compose".to_string(), "-p".to_string(), project_name];
    args.extend(compose_files);
    args.extend(vec![
        "exec".to_string(),
        "-T".to_string(),
        "pg".to_string(),
        "sh".to_string(),
        "-lc".to_string(),
        probe_script.to_string(),
    ]);

    let status = Command::new("docker")
        .args(&args)
        .current_dir(&dir)
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .await?;

    if !status.success() {
        anyhow::bail!(
            "postgres readiness probe failed after 60 seconds (status: {})",
            status
        );
    }

    debug("postgres is ready");

    Ok(())
}

/// Wait for redis to be ready by executing redis-cli ping inside a container.
/// Uses docker compose exec to probe the redis service.
pub async fn wait_for_redis_ready(app: &str) -> Result<()> {
    let dir = app_dir(app);

    if !dir.exists() {
        anyhow::bail!("App directory not found: {}", dir.display());
    }

    let mut compose_files = vec!["-f".to_string(), "compose.yml".to_string()];
    compose_files.push("-f".to_string());
    compose_files.push("compose.redis.yml".to_string());

    let project_name = format!("{}-acc", app);
    debug(&format!(
        "waiting for redis to be ready (project: {}, timeout: 60s)",
        project_name
    ));

    // Build the probe command: redis-cli ping with retry loop
    let probe_script = "for i in $(seq 1 60); do redis-cli -h 127.0.0.1 ping | grep -q PONG && exit 0; sleep 1; done; exit 1";

    let mut args = vec!["compose".to_string(), "-p".to_string(), project_name];
    args.extend(compose_files);
    args.extend(vec![
        "exec".to_string(),
        "-T".to_string(),
        "redis".to_string(),
        "sh".to_string(),
        "-lc".to_string(),
        probe_script.to_string(),
    ]);

    let status = Command::new("docker")
        .args(&args)
        .current_dir(&dir)
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .await?;

    if !status.success() {
        anyhow::bail!(
            "redis readiness probe failed after 60 seconds (status: {})",
            status
        );
    }

    debug("redis is ready");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_write_base_compose_file() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let dir_path = temp_dir.path();
        let image = "registry.example.com/testapp";
        let network = "traefik_proxy";
        write_base_compose_file(dir_path, image, network).await?;
        let compose_path = dir_path.join("compose.yml");
        assert!(compose_path.exists(), "compose.yml should be created");
        let content = fs::read_to_string(&compose_path).await?;
        let expected = r#"
services:
  base:
    image: registry.example.com/testapp:latest
    restart: unless-stopped
    env_file: [.env]
    networks: [traefik_proxy]
    profiles: ["_template"]
networks:
  traefik_proxy:
    external: true
    name: traefik_proxy
"#;
        assert_eq!(
            content, expected,
            "Compose file content should match expected output"
        );
        Ok(())
    }

    #[test]
    fn test_build_migration_args() {
        use std::collections::HashMap;

        // Create a test config with deterministic ordering by using a single env var
        let mut env_vars = HashMap::new();
        env_vars.insert("RAILS_ENV".to_string(), "production".to_string());

        let cfg = HLConfig {
            app: "testapp".to_string(),
            image: "registry.example.com/testapp".to_string(),
            domain: "testapp.example.com".to_string(),
            service_port: 3000,
            resolver: "myresolver".to_string(),
            network: "traefik_proxy".to_string(),
            platforms: "linux/amd64".to_string(),
            health: crate::config::HealthConfig {
                url: "http://testapp:3000/healthz".to_string(),
                interval: "2s".to_string(),
                timeout: "45s".to_string(),
            },
            migrations: crate::config::MigrationsConfig {
                command: vec!["bin/rails".to_string(), "db:migrate".to_string()],
                env: env_vars,
            },
            secrets: vec![],
        };

        let image_tag = "registry.example.com/testapp:abc1234";
        let env_path = "/home/user/prj/apps/testapp/.env";
        let args = build_migration_args(&cfg, image_tag, env_path);
        let result = args.join(" ");
        let expected = "run --rm --env-file /home/user/prj/apps/testapp/.env -e RAILS_ENV=production --network traefik_proxy registry.example.com/testapp:abc1234 bin/rails db:migrate";

        assert_eq!(
            result, expected,
            "Migration command should match expected output"
        );
    }

    #[tokio::test]
    async fn test_write_process_compose_files_with_procfile() -> Result<()> {
        use std::collections::HashMap;

        let temp_dir = TempDir::new()?;
        let dir_path = temp_dir.path();

        let mut processes = HashMap::new();
        processes.insert(
            "web".to_string(),
            "bundle exec rails server -p $PORT".to_string(),
        );
        processes.insert(
            "worker".to_string(),
            "bundle exec sidekiq -C config/sidekiq.yml".to_string(),
        );

        write_process_compose_files(dir_path, Some(&processes), "testapp", "myresolver").await?;

        // Check web compose file
        let web_path = dir_path.join("compose.web.yml");
        assert!(web_path.exists(), "compose.web.yml should be created");
        let web_content = fs::read_to_string(&web_path).await?;
        let expected_web = r#"
services:
  web:
    extends:
      file: ./compose.yml
      service: base

    container_name: testapp
    labels:
      traefik.enable: true
      traefik.http.routers.testapp.rule: Host(`${DOMAIN}`)
      traefik.http.routers.testapp.entrypoints: websecure
      traefik.http.routers.testapp.tls.certresolver: myresolver
      traefik.http.services.testapp.loadbalancer.server.port: ${SERVICE_PORT}
    command: ["bundle","exec","rails","server","-p","$PORT"]
"#;
        assert_eq!(
            web_content, expected_web,
            "Web compose content should match"
        );

        // Check worker compose file
        let worker_path = dir_path.join("compose.worker.yml");
        assert!(worker_path.exists(), "compose.worker.yml should be created");
        let worker_content = fs::read_to_string(&worker_path).await?;
        let expected_worker = r#"
services:
  worker:
    extends:
      file: ./compose.yml
      service: base

    command: ["bundle","exec","sidekiq","-C","config/sidekiq.yml"]
"#;
        assert_eq!(
            worker_content, expected_worker,
            "Worker compose content should match"
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_write_process_compose_files_without_procfile() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let dir_path = temp_dir.path();

        write_process_compose_files(dir_path, None, "testapp", "myresolver").await?;

        // Check default web compose file
        let web_path = dir_path.join("compose.web.yml");
        assert!(web_path.exists(), "compose.web.yml should be created");
        let web_content = fs::read_to_string(&web_path).await?;
        let expected_web = r#"
services:
  web:
    extends:
      file: ./compose.yml
      service: base

    container_name: testapp
    labels:
      traefik.enable: true
      traefik.http.routers.testapp.rule: Host(`${DOMAIN}`)
      traefik.http.routers.testapp.entrypoints: websecure
      traefik.http.routers.testapp.tls.certresolver: myresolver
      traefik.http.services.testapp.loadbalancer.server.port: ${SERVICE_PORT}
"#;
        assert_eq!(
            web_content, expected_web,
            "Default web compose content should match"
        );

        Ok(())
    }

    #[test]
    fn test_generate_process_compose_with_command() {
        let result = generate_process_compose(
            "worker",
            Some(&"bundle exec sidekiq".to_string()),
            "testapp",
            "myresolver",
        );
        let expected = r#"
services:
  worker:
    extends:
      file: ./compose.yml
      service: base

    command: ["bundle","exec","sidekiq"]
"#;
        assert_eq!(
            result, expected,
            "Process compose with command should match"
        );
    }

    #[test]
    fn test_generate_process_compose_without_command() {
        let result = generate_process_compose("web", None, "testapp", "myresolver");
        let expected = r#"
services:
  web:
    extends:
      file: ./compose.yml
      service: base

    container_name: testapp
    labels:
      traefik.enable: true
      traefik.http.routers.testapp.rule: Host(`${DOMAIN}`)
      traefik.http.routers.testapp.entrypoints: websecure
      traefik.http.routers.testapp.tls.certresolver: myresolver
      traefik.http.services.testapp.loadbalancer.server.port: ${SERVICE_PORT}
"#;
        assert_eq!(
            result, expected,
            "Process compose without command should match"
        );
    }

    #[test]
    fn test_generate_process_compose_with_complex_command() {
        let result = generate_process_compose(
            "release",
            Some(&"bundle exec rake db:migrate db:seed".to_string()),
            "testapp",
            "myresolver",
        );
        let expected = r#"
services:
  release:
    extends:
      file: ./compose.yml
      service: base

    command: ["bundle","exec","rake","db:migrate","db:seed"]
"#;
        assert_eq!(
            result, expected,
            "Complex command should be parsed correctly"
        );
    }
}
