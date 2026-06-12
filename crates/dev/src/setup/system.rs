use super::component::InstallState;
use super::context::SetupContext;
use anyhow::{Context, Result};
use std::path::Path;
use std::process::Command;

fn command_version(ctx: &SetupContext, command: &str, args: &[&str]) -> Result<Option<String>> {
    if !ctx.command_exists(command) {
        return Ok(None);
    }

    let output = Command::new(command).args(args).output()?;
    if !output.status.success() {
        return Ok(None);
    }

    Ok(Some(
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .next()
            .unwrap_or("")
            .trim()
            .to_string(),
    ))
}

fn require_command(ctx: &SetupContext, component: &str, command: &str) -> Result<()> {
    if ctx.command_exists(command) {
        return Ok(());
    }

    anyhow::bail!(
        "{} requires `{}` but it was not found in PATH. Install prerequisites with `dev setup run system_packages` or add `{}` to PATH.",
        component,
        command,
        command
    )
}

fn execute_shell(ctx: &SetupContext, component: &str, script: &str) -> Result<()> {
    ctx.execute(component, Command::new("sh").arg("-c").arg(script))
}

fn user_home() -> Result<String> {
    std::env::var("HOME").context("HOME is not set")
}

fn log_applied(ctx: &SetupContext, component: &str, message: &str) {
    if !ctx.dry_run {
        ctx.log.ok(component, message);
    }
}

/// Detect system packages installation state
pub fn detect_system_packages(ctx: &SetupContext) -> Result<InstallState> {
    // Check for essential build tools
    let has_gcc = ctx.command_exists("gcc");
    let has_make = ctx.command_exists("make");
    let has_git = ctx.command_exists("git");

    if has_gcc && has_make && has_git {
        Ok(InstallState::Installed {
            version: None,
            details: vec!["build-essential and core tools present".to_string()],
        })
    } else {
        let mut reasons = Vec::new();
        if !has_gcc {
            reasons.push("gcc not found".to_string());
        }
        if !has_make {
            reasons.push("make not found".to_string());
        }
        if !has_git {
            reasons.push("git not found".to_string());
        }

        if reasons.is_empty() {
            Ok(InstallState::NotInstalled)
        } else {
            Ok(InstallState::Partial { reasons })
        }
    }
}

/// Install system packages
pub fn install_system_packages(ctx: &SetupContext) -> Result<()> {
    let component = "system_packages";

    ctx.log.ok(component, "Updating package lists");
    ctx.execute(
        component,
        std::process::Command::new("sudo").arg("apt").arg("update"),
    )?;

    ctx.log.ok(component, "Installing system dependencies");

    let packages = vec![
        "build-essential",
        "libssl-dev",
        "libffi-dev",
        "libglib2.0-0",
        "libsm6",
        "libxext6",
        "libxrender-dev",
        "libxslt1.1",
        "libxslt1-dev",
        "libxml2",
        "libxml2-dev",
        "libreadline-dev",
        "libbz2-dev",
        "liblzma-dev",
        "wget",
        "curl",
        "cmake",
        "make",
        "libsqlite3-dev",
        "nano",
        "git",
        "git-lfs",
    ];

    ctx.execute(
        component,
        std::process::Command::new("sudo")
            .arg("apt")
            .arg("install")
            .arg("-y")
            .args(&packages),
    )?;

    log_applied(ctx, component, "System packages installed successfully");
    Ok(())
}

/// Detect Git LFS
pub fn detect_git_lfs(ctx: &SetupContext) -> Result<InstallState> {
    if let Some(version) = command_version(ctx, "git-lfs", &["--version"])? {
        Ok(InstallState::Installed {
            version: Some(version),
            details: vec![],
        })
    } else {
        Ok(InstallState::NotInstalled)
    }
}

/// Install Git LFS
pub fn install_git_lfs(ctx: &SetupContext) -> Result<()> {
    let component = "git_lfs";

    require_command(ctx, component, "git")?;

    ctx.log.ok(component, "Initializing Git LFS");
    ctx.execute(component, Command::new("git").arg("lfs").arg("install"))?;

    log_applied(ctx, component, "Git LFS initialized successfully");
    Ok(())
}

/// Detect uv
pub fn detect_uv(ctx: &SetupContext) -> Result<InstallState> {
    if let Some(version) = command_version(ctx, "uv", &["--version"])? {
        Ok(InstallState::Installed {
            version: Some(version),
            details: vec![],
        })
    } else {
        Ok(InstallState::NotInstalled)
    }
}

/// Install uv
pub fn install_uv(ctx: &SetupContext) -> Result<()> {
    let component = "uv";

    require_command(ctx, component, "curl")?;

    ctx.log.ok(component, "Installing uv");
    execute_shell(
        ctx,
        component,
        "curl -LsSf https://astral.sh/uv/install.sh | sh",
    )?;

    log_applied(ctx, component, "uv installed successfully");
    Ok(())
}

/// Detect rustup
pub fn detect_rustup(ctx: &SetupContext) -> Result<InstallState> {
    if let Some(version) = command_version(ctx, "rustup", &["--version"])? {
        Ok(InstallState::Installed {
            version: Some(version),
            details: vec![],
        })
    } else {
        Ok(InstallState::NotInstalled)
    }
}

/// Install rustup
pub fn install_rustup(ctx: &SetupContext) -> Result<()> {
    let component = "rustup";

    require_command(ctx, component, "curl")?;

    ctx.log.ok(component, "Installing Rust via rustup");
    execute_shell(
        ctx,
        component,
        "curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y",
    )?;

    log_applied(ctx, component, "Rust installed successfully");
    Ok(())
}

/// Detect Node.js
pub fn detect_node(ctx: &SetupContext) -> Result<InstallState> {
    // Try direct command first
    if let Some(version) = command_version(ctx, "node", &["--version"])? {
        return Ok(InstallState::Installed {
            version: Some(version),
            details: vec![],
        });
    }

    // Check if NVM is installed and has Node versions
    let home = user_home()?;
    let nvm_dir = format!("{}/.nvm/versions/node", home);
    if Path::new(&nvm_dir).exists() {
        // NVM installed but node not in PATH
        return Ok(InstallState::Partial {
            reasons: vec!["NVM installed but node not in PATH (source ~/.nvm/nvm.sh)".to_string()],
        });
    }

    Ok(InstallState::NotInstalled)
}

/// Install Node.js via nvm
pub fn install_node(ctx: &SetupContext) -> Result<()> {
    let component = "node";

    require_command(ctx, component, "curl")?;

    ctx.log.ok(component, "Installing nvm");

    // Install nvm
    execute_shell(
        ctx,
        component,
        "curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/v0.39.5/install.sh | bash",
    )?;

    ctx.log.ok(component, "Installing Node.js with nvm");

    // Install Node.js via nvm
    let home = user_home()?;
    let nvm_script = format!(
        "export NVM_DIR=\"$HOME/.nvm\" && [ -s \"$NVM_DIR/nvm.sh\" ] && . \"$NVM_DIR/nvm.sh\" && nvm install {} && nvm use {}",
        ctx.config.node_version, ctx.config.node_version
    );

    ctx.execute(
        component,
        Command::new("bash")
            .arg("-c")
            .arg(&nvm_script)
            .env("HOME", home),
    )?;

    log_applied(
        ctx,
        component,
        &format!("Node.js {} installed successfully", ctx.config.node_version),
    );
    Ok(())
}

/// Detect pnpm
pub fn detect_pnpm(ctx: &SetupContext) -> Result<InstallState> {
    // Try direct command first
    if let Some(version) = command_version(ctx, "pnpm", &["--version"])? {
        return Ok(InstallState::Installed {
            version: Some(version),
            details: vec![],
        });
    }

    // Check if pnpm is installed in common locations
    let home = user_home()?;
    let pnpm_paths = vec![
        format!("{}/.local/share/pnpm/pnpm", home),
        format!("{}/Library/pnpm/pnpm", home),
    ];

    for path in pnpm_paths {
        if Path::new(&path).exists() {
            return Ok(InstallState::Partial {
                reasons: vec!["pnpm installed but not in PATH".to_string()],
            });
        }
    }

    Ok(InstallState::NotInstalled)
}

/// Install pnpm
pub fn install_pnpm(ctx: &SetupContext) -> Result<()> {
    let component = "pnpm";

    require_command(ctx, component, "curl")?;

    // Check if node is available via NVM
    let home = user_home()?;
    let nvm_script = format!("{}/.nvm/nvm.sh", home);

    let has_node = if Path::new(&nvm_script).exists() {
        // Try with NVM loaded
        let check_cmd = "export NVM_DIR=\"$HOME/.nvm\" && [ -s \"$NVM_DIR/nvm.sh\" ] && . \"$NVM_DIR/nvm.sh\" && command -v node".to_string();
        Command::new("bash")
            .arg("-c")
            .arg(&check_cmd)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    } else {
        ctx.command_exists("node")
    };

    if !has_node {
        anyhow::bail!("Node.js is required but not installed");
    }

    ctx.log.ok(component, "Installing pnpm");

    // Install pnpm with NVM environment loaded if needed
    let install_cmd = if Path::new(&nvm_script).exists() {
        "export NVM_DIR=\"$HOME/.nvm\" && [ -s \"$NVM_DIR/nvm.sh\" ] && . \"$NVM_DIR/nvm.sh\" && curl -fsSL https://get.pnpm.io/install.sh | sh -".to_string()
    } else {
        "curl -fsSL https://get.pnpm.io/install.sh | sh -".to_string()
    };

    ctx.execute(
        component,
        Command::new("bash")
            .arg("-c")
            .arg(&install_cmd)
            .env("HOME", &home),
    )?;

    log_applied(ctx, component, "pnpm installed successfully");
    if !ctx.dry_run {
        ctx.log.warn(
            component,
            "Add pnpm to PATH: export PATH=\"$HOME/.local/share/pnpm:$PATH\"",
        );
    }
    Ok(())
}

/// Detect PM2
pub fn detect_pm2(ctx: &SetupContext) -> Result<InstallState> {
    if let Some(version) = command_version(ctx, "pm2", &["--version"])? {
        let service_exists = Path::new("/etc/systemd/system/pm2-resurrect.service").exists();

        if service_exists {
            Ok(InstallState::Installed {
                version: Some(version),
                details: vec!["systemd service configured".to_string()],
            })
        } else {
            Ok(InstallState::Partial {
                reasons: vec!["pm2 installed but systemd service not configured".to_string()],
            })
        }
    } else {
        Ok(InstallState::NotInstalled)
    }
}

/// Install PM2
pub fn install_pm2(ctx: &SetupContext) -> Result<()> {
    let component = "pm2";

    require_command(ctx, component, "pnpm")?;

    ctx.log.ok(component, "Installing PM2");

    ctx.execute(
        component,
        Command::new("pnpm").arg("install").arg("-g").arg("pm2"),
    )?;

    log_applied(ctx, component, "PM2 installed successfully");

    // Install systemd service
    if super::templates::detect_pm2_service()? {
        ctx.log
            .ok(component, "PM2 systemd service already installed");
    } else {
        ctx.log.ok(component, "Installing PM2 systemd service");
        super::templates::install_pm2_service(ctx)?;
    }

    Ok(())
}
