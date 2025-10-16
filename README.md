# `hl` - Homelab Deploy Tool

## Building

```bash
# Development build
cargo build

# Optimized release build
cross build --target x86_64-unknown-linux-gnu --release
```

## Usage

```bash
# Deploy an application
hl deploy --app myapp --sha abc123def --branch master

# Initialize a new app
hl init --app myapp --image ghcr.io/user/myapp --domain app.example.com --port 3000

# Rollback to a previous version
hl rollback myapp abc123d

# Manage secrets
hl secrets set myapp KEY=value
hl secrets ls myapp
```
