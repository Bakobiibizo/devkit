use super::context::SetupContext;
use anyhow::Result;
use std::path::PathBuf;

/// PM2 startup script template
pub const PM2_STARTUP_SCRIPT: &str = r#"#!/usr/bin/env bash

set -euo pipefail

# Create log directory if it doesn't exist
LOG_DIR="$HOME/logs"
mkdir -p "$LOG_DIR"
LOG_FILE="$LOG_DIR/pm2-startup.log"

exec >>"$LOG_FILE" 2>&1
echo "==== PM2 startup invoked at $(date) ===="

# Wait for network to be up a bit (DNS, filesystems)
sleep 5

# Prepare PATH for user-scoped package managers
export PATH="$HOME/.local/bin:$HOME/bin:$PATH"
if [ -d "$HOME/.local/share/pnpm" ]; then
  export PATH="$HOME/.local/share/pnpm:$PATH"
fi

# Try to load NVM if present
if [ -s "$HOME/.nvm/nvm.sh" ]; then
  # shellcheck disable=SC1090
  . "$HOME/.nvm/nvm.sh"
  # Use default alias if available, otherwise attempt to use the highest installed version
  if command -v nvm >/dev/null 2>&1; then
    if nvm alias default >/dev/null 2>&1; then
      nvm use --silent default || true
    else
      latest_node_dir=$(ls -1d "$HOME/.nvm/versions/node"/* 2>/dev/null | sort -V | tail -n1 || true)
      if [ -n "${latest_node_dir:-}" ]; then
        export PATH="$latest_node_dir/bin:$PATH"
      fi
    fi
  fi
fi

echo "PATH: $PATH"

# Verify Node.js is available
if ! command -v node >/dev/null 2>&1; then
  echo "Error: Node.js not found in PATH"
  exit 1
fi

# Locate PM2
PM2_BIN="$(command -v pm2 || true)"
if [ -z "$PM2_BIN" ]; then
  # Common pnpm local install path fallback
  if [ -x "$HOME/.local/share/pnpm/pm2" ]; then
    PM2_BIN="$HOME/.local/share/pnpm/pm2"
  fi
fi

if [ -z "$PM2_BIN" ]; then
  echo "Error: pm2 not found in PATH"
  exit 1
fi

echo "Using pm2 at: $PM2_BIN"

# Resurrect previous processes
if ! "$PM2_BIN" resurrect; then
  echo "Failed to resurrect PM2 at $(date)"
  exit 1
fi

# Save current state
if ! "$PM2_BIN" save; then
  echo "Failed to save PM2 state at $(date)"
  exit 1
fi

echo "PM2 has been successfully resurrected at $(date)"
"#;

/// Generate PM2 systemd service file content
pub fn generate_pm2_service(user: &str, home: &str, script_path: &str) -> String {
    format!(
        r#"[Unit]
Description=PM2 Process Manager Resurrect
After=network.target

[Service]
Type=oneshot
User={user}
Environment=HOME={home}
WorkingDirectory={home}
ExecStart={script_path}
RemainAfterExit=yes

[Install]
WantedBy=multi-user.target
"#,
        user = user,
        home = home,
        script_path = script_path
    )
}

/// Install PM2 systemd service
pub fn install_pm2_service(ctx: &SetupContext) -> Result<()> {
    let component = "pm2_service";

    // Get user and home directory
    let user = std::env::var("USER").unwrap_or_else(|_| "root".to_string());
    let home = std::env::var("HOME")?;

    // Create setup directory
    let setup_dir = PathBuf::from(&home).join("setup");
    if !setup_dir.exists() {
        std::fs::create_dir_all(&setup_dir)?;
    }

    // Write startup script
    let script_path = setup_dir.join("pm2-startup.sh");
    ctx.log.ok(component, &format!("Creating startup script at {}", script_path.display()));

    if !ctx.dry_run {
        std::fs::write(&script_path, PM2_STARTUP_SCRIPT)?;
        
        // Make executable
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&script_path)?.permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&script_path, perms)?;
        }
    } else {
        ctx.log.dry_run(component, &format!("Write startup script to {}", script_path.display()));
        ctx.log.dry_run(component, "chmod +x pm2-startup.sh");
    }

    // Write systemd service file
    let service_content = generate_pm2_service(&user, &home, script_path.to_str().unwrap());
    let service_path = "/etc/systemd/system/pm2-resurrect.service";

    ctx.log.ok(component, &format!("Creating systemd service at {}", service_path));

    if !ctx.dry_run {
        let temp_path = format!("/tmp/pm2-resurrect.service.{}", std::process::id());
        std::fs::write(&temp_path, service_content)?;

        // Move to systemd directory with sudo
        ctx.execute(
            component,
            std::process::Command::new("sudo")
                .arg("mv")
                .arg(&temp_path)
                .arg(service_path),
        )?;
    } else {
        ctx.log.dry_run(component, &format!("Write systemd service to {}", service_path));
    }

    // Reload systemd
    ctx.log.ok(component, "Reloading systemd daemon");
    ctx.execute(
        component,
        std::process::Command::new("sudo")
            .arg("systemctl")
            .arg("daemon-reload"),
    )?;

    // Enable service
    ctx.log.ok(component, "Enabling pm2-resurrect service");
    ctx.execute(
        component,
        std::process::Command::new("sudo")
            .arg("systemctl")
            .arg("enable")
            .arg("pm2-resurrect"),
    )?;

    // Start service
    ctx.log.ok(component, "Starting pm2-resurrect service");
    ctx.execute(
        component,
        std::process::Command::new("sudo")
            .arg("systemctl")
            .arg("start")
            .arg("pm2-resurrect"),
    )?;

    ctx.log.ok(component, "PM2 systemd service installed successfully");
    ctx.log.ok(component, "Check status with: systemctl status pm2-resurrect");

    Ok(())
}

/// Detect if PM2 systemd service is installed
pub fn detect_pm2_service() -> Result<bool> {
    let service_path = std::path::Path::new("/etc/systemd/system/pm2-resurrect.service");
    Ok(service_path.exists())
}
