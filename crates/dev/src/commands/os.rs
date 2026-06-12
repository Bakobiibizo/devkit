use std::fs;

use anyhow::{Context, Result, anyhow};

use crate::cli::OsCommand;
use crate::dispatch::AppState;

pub(crate) fn handle(state: &AppState, command: OsCommand) -> Result<()> {
    let config_path = &state.config_path;

    match command {
        OsCommand::Show => {
            let current_os = std::env::consts::OS;
            println!("Detected OS: {}", current_os);
            println!(
                "Config path: {} ({})",
                config_path,
                state.config_source.as_str()
            );

            let config_dir = config_path
                .parent()
                .ok_or_else(|| anyhow!("cannot determine config directory"))?;

            let linux_path = config_dir.join("config.linux.toml");
            let windows_path = config_dir.join("config.windows.toml");

            println!(
                "Linux config:   {} ({})",
                linux_path,
                if linux_path.exists() {
                    "exists"
                } else {
                    "not found"
                }
            );
            println!(
                "Windows config: {} ({})",
                windows_path,
                if windows_path.exists() {
                    "exists"
                } else {
                    "not found"
                }
            );

            Ok(())
        }
        OsCommand::Linux => os_switch(state, "linux"),
        OsCommand::Windows => os_switch(state, "windows"),
    }
}

fn os_switch(state: &AppState, target_os: &str) -> Result<()> {
    let template_path = format!("config/tauri.config.{}.toml", target_os);
    let content = crate::templates::get_string(&template_path)
        .with_context(|| format!("loading embedded template for {}", target_os))?;

    let config_dir = state
        .config_path
        .parent()
        .ok_or_else(|| anyhow!("cannot determine config directory"))?;

    let platform_config = config_dir.join(format!("config.{}.toml", target_os));

    if state.ctx.dry_run {
        println!(
            "[dry-run] would write {} config to {}",
            target_os, platform_config
        );
        return Ok(());
    }

    fs::create_dir_all(config_dir.as_std_path())
        .with_context(|| format!("creating directory {}", config_dir))?;

    fs::write(platform_config.as_std_path(), &content)
        .with_context(|| format!("writing {}", platform_config))?;

    println!("Wrote {} config to {}", target_os, platform_config);
    println!(
        "This config will be preferred when running on {}.",
        target_os
    );

    Ok(())
}
