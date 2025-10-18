# hl - Copilot Instructions

## Project Overview

**hl** is a deterministic, single-host deployment tool written in Rust that orchestrates "git-push deploys" using Docker, systemd, and Traefik. Think Heroku-style deploys without orchestration complexity.

**Core Philosophy:** Explicit over automatic, boring primitives over magic, per-app isolation.

## Architecture & Data Flow

### Deploy Pipeline (triggered by git post-receive hook)

1. **Export** commit via `git archive` to ephemeral temp directory (`src/git.rs`)
2. **Build** using Docker Buildx from exported context, push with tags: `:sha`, `:branch-sha`, `:latest` (`src/docker.rs`)
3. **Migrate** by running one-off container with new image tag (`src/docker.rs::run_migrations`)
4. **Retag** `:latest` → new SHA via pull/tag/push sequence (`src/docker.rs::retag_latest`)
5. **Restart** via systemd which invokes `docker compose up -d` (`src/systemd.rs`)
6. **Health-gate** using curl in Docker network until timeout (`src/health.rs`)

### Key Components

- **Commands** (`src/commands/`): CLI subcommands with async execution
- **Config** (`src/config.rs`): Server-owned `hl.yml` per app at `/home/<user>/prj/apps/<app>/`
- **Docker** (`src/docker.rs`): Build, push, retag, migrations, compose operations
- **Git** (`src/git.rs`): Deterministic commit export via `git archive | tar`
- **Systemd** (`src/systemd.rs`): Unit file generation with auto-discovered `compose.*.yml` files
- **Health** (`src/health.rs`): In-network health checks using ephemeral curl containers

### File System Layout (per app)

```
/home/<user>/prj/apps/<app>/
  compose.yml                  # Main app service with Traefik labels
  compose.postgres.yml         # Optional accessory (auto-discovered)
  compose.redis.yml            # Optional accessory (auto-discovered)
  .env                         # Secrets (0600 permissions)
  hl.yml                       # Server-owned config (see src/config.rs)
  pgdata/                      # Volumes
```

```
/home/<user>/prj/git/<app>.git/  # Bare repo with post-receive hook
```

## Critical Patterns

### Error Handling

- Use `anyhow::Result` and `anyhow::bail!` for all fallible operations
- Validate file existence before operations (see `src/docker.rs::build_and_push`)
- Context-rich errors: `anyhow::Context` trait for wrapping errors

### Async Execution

- All commands use `tokio::process::Command` with `Stdio::inherit` for live output
- Check `.status().await?` and verify `status.success()`
- Background processes NOT supported - all operations are synchronous/blocking

### Logging Convention

- `log::debug()` for verbose-only output (controlled by global `VERBOSE` atomic bool)
- `log::log()` for progress steps (e.g., "building", "retagging")
- `log::ok()` for success messages (e.g., "deploy complete")
- Never use `println!` directly - always go through `src/log.rs` functions

### Configuration

- Config deserialization uses `serde_yaml` with `#[serde(rename_all = "camelCase")]`
- Defaults via `#[serde(default = "default_*")]` functions (see `src/config.rs`)
- Path helpers: `app_dir()`, `hl_root()`, `hl_git_root()`, `env_file()`

### Docker Operations

- **Always** use explicit tag lists in builds (sha, branch-sha, latest)
- Retag operations: pull source → tag → push sequence (cannot retag remotely)
- Build contexts are ephemeral temp directories from `git archive`
- Multi-platform builds via `--platform` flag (default: `linux/amd64`)

### Systemd Integration

- Units named `app-<appname>.service`
- Working directory: `/home/<user>/prj/apps/<app>`
- Auto-discover all `compose.*.yml` files and chain with `-f` flags
- Use `systemctl --user` commands for non-root deployments

### Health Checks

- Run `curl` inside a Docker container on the app's network (not host)
- URL format: `http://<service-name>:<port>/path` (internal networking)
- Parse durations: `"2s"`, `"45s"`, `"100ms"` → milliseconds (`config::parse_duration`)
- Retry with interval until timeout, then fail

## Development Workflow

### Building

```bash
./build.sh              # Release build with optimizations
cargo build             # Debug build
cargo build --release   # Manual release build
```

### Testing on Server

```bash
# Install locally
cp target/release/hl /usr/local/bin/hl

# Initialize app
hl init --app myapp --image registry.example.com/myapp --domain myapp.example.com --port 8080

# Manually trigger deploy
hl deploy --app myapp --sha <commit-sha> --branch master
```

### Adding New Commands

1. Create module in `src/commands/<name>.rs`
2. Define `pub struct <Name>Args` with `#[derive(Args)]`
3. Implement `pub async fn execute(args: <Name>Args) -> Result<()>`
4. Add to `src/commands/mod.rs` and `src/main.rs::Commands` enum
5. Use existing helpers from `src/config.rs`, `src/docker.rs`, etc.

## Common Gotchas

- **Migrations hang?** Database container must be running first. No auto-start validation yet.
- **Health checks fail?** Verify service name matches container name in health URL (e.g., `http://myapp:8080`)
- **Compose files not found?** Systemd unit scans for `compose.*.yml` - restart service after adding accessories
- **Registry push fails?** Server must be logged into Docker registry (`docker login`)
- **Temp dir cleanup?** Export commit creates temp dirs - cleanup is best-effort (see `deploy.rs` end)

## Key Files to Reference

- `src/commands/deploy.rs` - Main deploy flow orchestration
- `src/docker.rs` - All Docker CLI interactions
- `src/config.rs` - Config schema and path conventions
- `src/systemd.rs` - Unit file generation logic
- `README.md` - User-facing documentation and rationale
