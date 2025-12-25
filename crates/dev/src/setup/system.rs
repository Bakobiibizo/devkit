use super::component::InstallState;
use super::context::SetupContext;
use anyhow::Result;

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
        std::process::Command::new("sudo")
            .arg("apt")
            .arg("update"),
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

    ctx.log.ok(component, "System packages installed successfully");
    Ok(())
}

/// Detect Git LFS
pub fn detect_git_lfs(ctx: &SetupContext) -> Result<InstallState> {
    if ctx.command_exists("git-lfs") {
        // Check if git-lfs is initialized
        let output = std::process::Command::new("git")
            .arg("lfs")
            .arg("version")
            .output()?;

        if output.status.success() {
            let version = String::from_utf8_lossy(&output.stdout);
            let version_str = version.lines().next().unwrap_or("").to_string();
            Ok(InstallState::Installed {
                version: Some(version_str),
                details: vec![],
            })
        } else {
            Ok(InstallState::Partial {
                reasons: vec!["git-lfs installed but not initialized".to_string()],
            })
        }
    } else {
        Ok(InstallState::NotInstalled)
    }
}

/// Install Git LFS
pub fn install_git_lfs(ctx: &SetupContext) -> Result<()> {
    let component = "git_lfs";

    if !ctx.command_exists("git") {
        anyhow::bail!("git is required but not installed");
    }

    ctx.log.ok(component, "Initializing Git LFS");
    ctx.execute(
        component,
        std::process::Command::new("git")
            .arg("lfs")
            .arg("install"),
    )?;

    ctx.log.ok(component, "Git LFS initialized successfully");
    Ok(())
}

/// Detect uv
pub fn detect_uv(ctx: &SetupContext) -> Result<InstallState> {
    if ctx.command_exists("uv") {
        let output = std::process::Command::new("uv")
            .arg("--version")
            .output()?;

        if output.status.success() {
            let version = String::from_utf8_lossy(&output.stdout);
            let version_str = version.trim().to_string();
            Ok(InstallState::Installed {
                version: Some(version_str),
                details: vec![],
            })
        } else {
            Ok(InstallState::NotInstalled)
        }
    } else {
        Ok(InstallState::NotInstalled)
    }
}

/// Install uv
pub fn install_uv(ctx: &SetupContext) -> Result<()> {
    let component = "uv";

    if !ctx.command_exists("curl") {
        anyhow::bail!("curl is required but not installed");
    }

    ctx.log.ok(component, "Installing uv");
    
    if ctx.dry_run {
        ctx.log.dry_run(component, "curl -LsSf https://astral.sh/uv/install.sh | sh");
        return Ok(());
    }

    let output = std::process::Command::new("sh")
        .arg("-c")
        .arg("curl -LsSf https://astral.sh/uv/install.sh | sh")
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Failed to install uv: {}", stderr);
    }

    ctx.log.ok(component, "uv installed successfully");
    Ok(())
}

/// Detect rustup
pub fn detect_rustup(ctx: &SetupContext) -> Result<InstallState> {
    if ctx.command_exists("rustup") {
        let output = std::process::Command::new("rustup")
            .arg("--version")
            .output()?;

        if output.status.success() {
            let version = String::from_utf8_lossy(&output.stdout);
            let version_str = version.lines().next().unwrap_or("").to_string();
            Ok(InstallState::Installed {
                version: Some(version_str),
                details: vec![],
            })
        } else {
            Ok(InstallState::NotInstalled)
        }
    } else {
        Ok(InstallState::NotInstalled)
    }
}

/// Install rustup
pub fn install_rustup(ctx: &SetupContext) -> Result<()> {
    let component = "rustup";

    if !ctx.command_exists("curl") {
        anyhow::bail!("curl is required but not installed");
    }

    ctx.log.ok(component, "Installing Rust via rustup");
    
    if ctx.dry_run {
        ctx.log.dry_run(component, "curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y");
        return Ok(());
    }

    let output = std::process::Command::new("sh")
        .arg("-c")
        .arg("curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y")
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Failed to install rustup: {}", stderr);
    }

    ctx.log.ok(component, "Rust installed successfully");
    Ok(())
}

/// Detect Node.js
pub fn detect_node(ctx: &SetupContext) -> Result<InstallState> {
    // Try direct command first
    if ctx.command_exists("node") {
        let output = std::process::Command::new("node")
            .arg("--version")
            .output()?;

        if output.status.success() {
            let version = String::from_utf8_lossy(&output.stdout);
            let version_str = version.trim().to_string();
            return Ok(InstallState::Installed {
                version: Some(version_str),
                details: vec![],
            });
        }
    }

    // Check if NVM is installed and has Node versions
    let home = std::env::var("HOME")?;
    let nvm_dir = format!("{}/.nvm/versions/node", home);
    if std::path::Path::new(&nvm_dir).exists() {
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

    if !ctx.command_exists("curl") {
        anyhow::bail!("curl is required but not installed");
    }

    ctx.log.ok(component, "Installing nvm");
    
    if ctx.dry_run {
        ctx.log.dry_run(component, "curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/v0.39.5/install.sh | bash");
        ctx.log.dry_run(component, &format!("nvm install {}", ctx.config.node_version));
        return Ok(());
    }

    // Install nvm
    let output = std::process::Command::new("sh")
        .arg("-c")
        .arg("curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/v0.39.5/install.sh | bash")
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Failed to install nvm: {}", stderr);
    }

    ctx.log.ok(component, "nvm installed, installing Node.js");

    // Install Node.js via nvm
    let home = std::env::var("HOME")?;
    let nvm_script = format!("export NVM_DIR=\"$HOME/.nvm\" && [ -s \"$NVM_DIR/nvm.sh\" ] && . \"$NVM_DIR/nvm.sh\" && nvm install {} && nvm use {}", 
        ctx.config.node_version, ctx.config.node_version);

    let output = std::process::Command::new("bash")
        .arg("-c")
        .arg(&nvm_script)
        .env("HOME", home)
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Failed to install Node.js: {}", stderr);
    }

    ctx.log.ok(component, &format!("Node.js {} installed successfully", ctx.config.node_version));
    Ok(())
}

/// Detect pnpm
pub fn detect_pnpm(ctx: &SetupContext) -> Result<InstallState> {
    // Try direct command first
    if ctx.command_exists("pnpm") {
        let output = std::process::Command::new("pnpm")
            .arg("--version")
            .output()?;

        if output.status.success() {
            let version = String::from_utf8_lossy(&output.stdout);
            let version_str = version.trim().to_string();
            return Ok(InstallState::Installed {
                version: Some(version_str),
                details: vec![],
            });
        }
    }

    // Check if pnpm is installed in common locations
    let home = std::env::var("HOME")?;
    let pnpm_paths = vec![
        format!("{}/.local/share/pnpm/pnpm", home),
        format!("{}/Library/pnpm/pnpm", home),
    ];

    for path in pnpm_paths {
        if std::path::Path::new(&path).exists() {
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

    if !ctx.command_exists("curl") {
        anyhow::bail!("curl is required but not installed");
    }

    // Check if node is available via NVM
    let home = std::env::var("HOME")?;
    let nvm_script = format!("{}/.nvm/nvm.sh", home);
    
    let has_node = if std::path::Path::new(&nvm_script).exists() {
        // Try with NVM loaded
        let check_cmd = format!(
            "export NVM_DIR=\"$HOME/.nvm\" && [ -s \"$NVM_DIR/nvm.sh\" ] && . \"$NVM_DIR/nvm.sh\" && command -v node"
        );
        std::process::Command::new("bash")
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
    
    if ctx.dry_run {
        ctx.log.dry_run(component, "curl -fsSL https://get.pnpm.io/install.sh | sh -");
        return Ok(());
    }

    // Install pnpm with NVM environment loaded if needed
    let install_cmd = if std::path::Path::new(&nvm_script).exists() {
        format!(
            "export NVM_DIR=\"$HOME/.nvm\" && [ -s \"$NVM_DIR/nvm.sh\" ] && . \"$NVM_DIR/nvm.sh\" && curl -fsSL https://get.pnpm.io/install.sh | sh -"
        )
    } else {
        "curl -fsSL https://get.pnpm.io/install.sh | sh -".to_string()
    };

    let output = std::process::Command::new("bash")
        .arg("-c")
        .arg(&install_cmd)
        .env("HOME", &home)
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Failed to install pnpm: {}", stderr);
    }

    ctx.log.ok(component, "pnpm installed successfully");
    ctx.log.warn(component, "Add pnpm to PATH: export PATH=\"$HOME/.local/share/pnpm:$PATH\"");
    Ok(())
}

/// Detect PM2
pub fn detect_pm2(ctx: &SetupContext) -> Result<InstallState> {
    if ctx.command_exists("pm2") {
        let output = std::process::Command::new("pm2")
            .arg("--version")
            .output()?;

        if output.status.success() {
            let version = String::from_utf8_lossy(&output.stdout);
            let version_str = version.trim().to_string();
            
            // Check if systemd service exists
            let service_exists = std::path::Path::new("/etc/systemd/system/pm2-resurrect.service").exists();
            
            if service_exists {
                Ok(InstallState::Installed {
                    version: Some(version_str),
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
    } else {
        Ok(InstallState::NotInstalled)
    }
}

/// Install PM2
pub fn install_pm2(ctx: &SetupContext) -> Result<()> {
    let component = "pm2";

    if !ctx.command_exists("pnpm") {
        anyhow::bail!("pnpm is required but not installed");
    }

    ctx.log.ok(component, "Installing PM2");
    
    if ctx.dry_run {
        ctx.log.dry_run(component, "pnpm install -g pm2");
    } else {
        let output = std::process::Command::new("pnpm")
            .arg("install")
            .arg("-g")
            .arg("pm2")
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Failed to install PM2: {}", stderr);
        }
    }

    ctx.log.ok(component, "PM2 installed successfully");
    
    // Install systemd service
    if super::templates::detect_pm2_service()? {
        ctx.log.ok(component, "PM2 systemd service already installed");
    } else {
        ctx.log.ok(component, "Installing PM2 systemd service");
        super::templates::install_pm2_service(ctx)?;
    }
    
    Ok(())
}
