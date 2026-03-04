# hl — Copilot Repository Instructions

## What This Repository Does

**hl** is a single-host "git-push deploy" CLI tool written in Rust. It orchestrates builds and deploys using Docker Buildx, systemd, and Traefik — Heroku-style deploys without an orchestrator. When a developer pushes to a bare Git repo on the server, a `post-receive` hook calls `hl deploy`, which exports the commit, builds/pushes a Docker image, runs migrations, retags `:latest`, restarts the app via systemd, and health-gates the deploy.

## Build, Test, and Lint

Always run these commands from the repository root. All three are required to pass before merging.

```bash
# Format check (CI enforces this)
cargo fmt --all -- --check

# Lints — warnings are treated as errors
cargo clippy --all-targets --all-features -- -D warnings

# Tests
cargo test --all --locked
```

To auto-fix formatting:

```bash
cargo fmt --all
```

To build the release binary:

```bash
cargo build --release   # produces target/release/hl
# or
./build.sh              # release build with size optimizations (LTO, strip)
```

**Formatting config:** `.rustfmt.toml` sets `max_width = 100` and `tab_spaces = 2`.

## Project Layout

```
src/
  main.rs                  # CLI entry point; defines Commands enum
  lib.rs                   # Library root; re-exports modules
  commands/                # One file per CLI subcommand
    deploy.rs              # Core deploy pipeline (export→build→migrate→retag→restart→health)
    init.rs                # Bootstrap app directory, compose.yml, hl.yml, systemd unit
    rollback.rs            # Retag :latest to a prior sha, restart, health-gate
    env.rs                 # Manage .env variables
    accessory.rs           # Add postgres/redis accessories
    restart.rs             # systemctl restart wrapper
    logs.rs                # docker compose logs wrapper
    teardown.rs            # Remove app files, services, and git repo
    mod.rs                 # Module declarations
  config.rs                # Config schema (hl.yml), path helpers, duration parsing
  docker.rs                # All Docker CLI interactions (build, push, retag, run)
  git.rs                   # git archive export to ephemeral temp dir
  systemd.rs               # Systemd unit file generation and management
  health.rs                # In-network health checks via ephemeral curl container
  log.rs                   # Logging helpers (debug/log/ok); controls VERBOSE global
  env.rs                   # .env file read/write helpers
  discovery.rs             # Auto-discovers compose.*.yml files
  procfile.rs              # Procfile parsing helpers
  units_spec_builder.rs    # Builds systemd unit specs from compose file list
.github/
  workflows/ci.yml         # CI: rustfmt + clippy + cargo test
  instructions/
    rust.instructions.md   # Path-scoped Copilot instructions for *.rs files
Cargo.toml                 # Dependencies and release profile
.rustfmt.toml              # Formatter config (max_width=100, tab_spaces=2)
AGENTS.md                  # Detailed agent/architecture reference
README.md                  # User-facing documentation
```

## Key Coding Conventions

- **Errors:** use `anyhow::Result`, `anyhow::bail!`, and `.context(...)` everywhere; no `unwrap()`/`expect()` in non-test code.
- **Async:** all commands are `async fn execute(args: ...) -> Result<()>` using `tokio`; use `Stdio::inherit` for live subprocess output.
- **Logging:** use `log::debug()` (verbose), `log::log()` (progress), `log::ok()` (success) from `src/log.rs` — never `println!`.
- **Config:** `hl.yml` is deserialized with `serde_yaml`; use `#[serde(rename_all = "camelCase")]` and `#[serde(default = "default_*")]` for defaults.
- **Paths:** use `config::app_dir()`, `config::hl_root()`, etc. — never hard-code paths.
- **Formatting:** 2-space indentation, max 100-character lines.

## Adding a New Subcommand

1. Create `src/commands/<name>.rs` with `pub struct <Name>Args` (`#[derive(Args)]`) and `pub async fn execute(args: <Name>Args) -> Result<()>`.
2. Declare it in `src/commands/mod.rs`.
3. Add a variant to the `Commands` enum in `src/main.rs` and wire it in the `match` block.

## Common Gotchas

- **Never** use `println!` — use `src/log.rs` helpers.
- Clippy warnings fail the build (`-D warnings`); fix all warnings before committing.
- The release profile uses LTO + strip; ensure the binary compiles cleanly with `cargo build --release`.
- Tests use `tempfile` and `serial_test` crates for filesystem isolation and test ordering.
