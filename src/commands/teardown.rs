use anyhow::Result;
use clap::Args;
use hl::{
    config::{app_dir, hl_git_root, systemd_dir},
    log::*,
    systemd::{reload_systemd_daemon, stop_app_target},
};
use tokio::fs;

#[derive(Args)]
pub struct TeardownArgs {
    /// Application name
    #[arg(long)]
    pub app: String,

    /// Skip confirmation prompt
    #[arg(long)]
    pub force: bool,
}

pub async fn execute(args: TeardownArgs) -> Result<()> {
    let app = &args.app;

    // Confirmation prompt unless --force is used
    if !args.force {
        log(&format!(
            "⚠️  This will permanently delete all data for app '{}':",
            app
        ));
        log("   - Stop all running services (web, workers, accessories)");
        log("   - Remove systemd unit files");
        log(&format!(
            "   - Remove git repository: ~/prj/git/{}.git",
            app
        ));
        log(&format!("   - Remove app directory: ~/prj/apps/{}", app));
        log("");
        log("Type the app name to confirm deletion:");

        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        let input = input.trim();

        if input != app {
            log("Aborted.");
            return Ok(());
        }
    }

    log(&format!("tearing down app: {}", app));

    // Step 1: Stop and disable the app target (this stops all services)
    stop_app_target(app).await?;

    // Step 2: Remove systemd unit files
    remove_systemd_units(app).await?;
    reload_systemd_daemon().await?;

    remove_git_repo(app).await?;
    remove_app_dir(app).await?;

    ok(&format!("app '{}' has been completely removed", app));

    Ok(())
}

async fn remove_systemd_units(app: &str) -> Result<()> {
    let systemd_path = systemd_dir();
    let unit_patterns = vec![
        format!("app-{}.target", app),
        format!("app-{}-*.service", app),
    ];

    debug(&format!(
        "removing systemd units from: {}",
        systemd_path.display()
    ));

    for pattern in unit_patterns {
        // Find all matching files
        let mut entries = fs::read_dir(&systemd_path).await?;
        while let Some(entry) = entries.next_entry().await? {
            let filename = entry.file_name();
            let filename_str = filename.to_string_lossy();

            // Check if filename matches the pattern
            if pattern.contains('*') {
                let prefix = pattern.split('*').next().unwrap();
                let suffix = pattern.split('*').last().unwrap();
                if filename_str.starts_with(prefix) && filename_str.ends_with(suffix) {
                    let path = entry.path();
                    debug(&format!("removing unit file: {}", path.display()));
                    fs::remove_file(&path).await?;
                }
            } else if filename_str == pattern {
                let path = entry.path();
                debug(&format!("removing unit file: {}", path.display()));
                fs::remove_file(&path).await?;
            }
        }
    }

    log("removed systemd unit files");

    Ok(())
}

async fn remove_git_repo(app: &str) -> Result<()> {
    let git_path = hl_git_root(app);

    if git_path.exists() {
        debug(&format!("removing git repository: {}", git_path.display()));
        fs::remove_dir_all(&git_path).await?;
        log(&format!("removed git repository: {}", git_path.display()));
    } else {
        debug(&format!(
            "git repository not found: {} (skipping)",
            git_path.display()
        ));
    }

    Ok(())
}

async fn remove_app_dir(app: &str) -> Result<()> {
    let app_path = app_dir(app);

    if app_path.exists() {
        debug(&format!("removing app directory: {}", app_path.display()));
        fs::remove_dir_all(&app_path).await?;
        log(&format!("removed app directory: {}", app_path.display()));
    } else {
        debug(&format!(
            "app directory not found: {} (skipping)",
            app_path.display()
        ));
    }

    Ok(())
}
