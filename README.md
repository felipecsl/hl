# `hl` - Homelab Deploy Tool

## Building

```bash
# Development build
cargo build

# Optimized release build
cross build --target x86_64-unknown-linux-gnu --release
```

The release profile is optimized for minimal binary size:

- LTO enabled
- Single codegen unit
- Stripped symbols
- Size optimization (`opt-level = "z"`)

## Binary Size Comparison

After building with `cargo build --release`, check the binary size:

```bash
ls -lh target/release/hl
```

Compare this to the Node.js standalone executable which was over 80MB.

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

## Dependencies

- `clap`: Command-line argument parsing
- `tokio`: Async runtime
- `serde` + `serde_yaml`: Config file parsing
- `anyhow`: Error handling
- `reqwest`: Health check HTTP requests
- `colored`: Terminal output coloring

## Further Size Optimization

If you need even smaller binaries, consider:

1. Using `upx` to compress the binary:

   ```bash
   upx --best --lzma target/release/hl
   ```

2. Cross-compile with musl for static linking:

   ```bash
   cargo build --release --target x86_64-unknown-linux-musl
   ```

3. Remove unused features from dependencies in `Cargo.toml`
