# `hl` your homelab app CLI

Goal: A CLI to spin up, manage, and monitor apps on a homelab server.

## Building

```bash
# Development build
cargo build

# Optimized release build
cross build --target x86_64-unknown-linux-gnu --release
```

## Deploying the tool

```bash
scp target/x86_64-unknown-linux-gnu/release/hl host:/home/felipecsl/.hl/bin/hl
```

## Usage

```bash
# Initialize a new app
hl init --app myapp --image ghcr.io/user/myapp --domain app.example.com --port 3000

git remote add production ssh://user@host/path/to/myapp.git

# Deploying an application happens automatically upon git push
git push production master

# Rollback to a previous version
hl rollback myapp gitsha

# Managing secrets
hl secrets set myapp KEY=value
hl secrets ls myapp
```
