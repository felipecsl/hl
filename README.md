# `hl` - your homelab app CLI

Ever wish you had your own private [Heroku](https://www.heroku.com/)?

`hl` is a command line tool to spin up, manage, deploy, and monitor apps on a homelab server, or
any remote host, really.

`hl` needs Docker and git on the remote host.

Apps are placed under `~/apps/<appname>` on the remote host.
Deploying an app happens automatically upon running `git push`.

Traefik is used as a reverse proxy, so make sure you have it set up beforehand. Traefik is not
currently managed by `hl` but may be in the future.

## Building

```bash
# Development build
cargo build

# Release build for linux x86_64
cross build --target x86_64-unknown-linux-gnu --release
```

## Deploying `hl` to your remote host

```bash
scp target/x86_64-unknown-linux-gnu/release/hl host:/home/youruser/.hl/bin/hl
```

## `hl` Wrapper script

You can use the script below to conveniently call `hl` on your remote host from your local machine.

```bash
#!/usr/bin/env bash
set -euo pipefail
REMOTE_USER="${REMOTE_USER:-youruser}"
REMOTE_HOST="${REMOTE_HOST:-your-remote-host.com}"
ssh "${REMOTE_USER}@${REMOTE_HOST}" "~/.hl/bin/hl $*"
```

## Usage

```bash
# Initialize a new app
hl init --app myapp --image ghcr.io/user/myapp --domain app.example.com --port 3000

git remote add production ssh://user@host/path/to/myapp.git

# Deploying to production
git push production master

# Rollback to a previous version
hl rollback myapp gitsha

# Managing secrets
hl secrets set myapp KEY=value
hl secrets ls myapp
```
