# Release Process

1. Make sure `main` is green and all changes intended for the release are merged.
2. In GitHub, open `Actions` → `Release` and trigger **Run workflow**.
3. Choose the semver bump (`patch`, `minor`, `major`) or set an explicit version override.
4. The `prepare` job will:
   - Bump the version in `Cargo.toml` using `cargo set-version`
   - Run `cargo test --locked`
   - Commit `Release vX.Y.Z` and create/push tag `vX.Y.Z`
5. GitHub automatically starts a second run (triggered by the tag push) that builds release artifacts for all targets and publishes a GitHub release with notes and archives.
6. Verify the release page to confirm the expected assets are attached.

If anything fails during preparation, fix the issue on `main` and re-run the workflow; the version bump is fully automated and will only land when the run succeeds.***
