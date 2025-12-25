use super::component::InstallState;
use super::context::SetupContext;
use anyhow::Result;

/// Detect zoxide
pub fn detect_zoxide(ctx: &SetupContext) -> Result<InstallState> {
    if ctx.command_exists("zoxide") {
        let output = std::process::Command::new("zoxide")
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

/// Install zoxide
pub fn install_zoxide(ctx: &SetupContext) -> Result<()> {
    let component = "zoxide";

    if !ctx.command_exists("cargo") {
        anyhow::bail!("cargo is required but not installed");
    }

    ctx.log.ok(component, "Installing zoxide via cargo");

    if ctx.dry_run {
        ctx.log.dry_run(component, "cargo install zoxide");
        return Ok(());
    }

    let output = std::process::Command::new("cargo")
        .arg("install")
        .arg("zoxide")
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Failed to install zoxide: {}", stderr);
    }

    ctx.log.ok(component, "zoxide installed successfully");
    ctx.log.warn(component, "Add 'eval \"$(zoxide init --cmd cd bash)\"' to your ~/.bashrc to enable");

    Ok(())
}

/// Detect atuin
pub fn detect_atuin(ctx: &SetupContext) -> Result<InstallState> {
    if ctx.command_exists("atuin") {
        let output = std::process::Command::new("atuin")
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

/// Install atuin
pub fn install_atuin(ctx: &SetupContext) -> Result<()> {
    let component = "atuin";

    if !ctx.command_exists("curl") {
        anyhow::bail!("curl is required but not installed");
    }

    ctx.log.ok(component, "Installing atuin");

    if ctx.dry_run {
        ctx.log.dry_run(component, "curl --proto '=https' --tlsv1.2 -LsSf https://setup.atuin.sh | sh");
        return Ok(());
    }

    let output = std::process::Command::new("sh")
        .arg("-c")
        .arg("curl --proto '=https' --tlsv1.2 -LsSf https://setup.atuin.sh | sh")
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Failed to install atuin: {}", stderr);
    }

    ctx.log.ok(component, "atuin installed successfully");

    Ok(())
}

/// Detect ngrok
pub fn detect_ngrok(ctx: &SetupContext) -> Result<InstallState> {
    if ctx.command_exists("ngrok") {
        let output = std::process::Command::new("ngrok")
            .arg("version")
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

/// Install ngrok
pub fn install_ngrok(ctx: &SetupContext) -> Result<()> {
    let component = "ngrok";

    ctx.log.ok(component, "Adding ngrok repository");

    if !ctx.dry_run {
        let output = std::process::Command::new("sh")
            .arg("-c")
            .arg("curl -s https://ngrok-agent.s3.amazonaws.com/ngrok.asc | sudo tee /etc/apt/trusted.gpg.d/ngrok.asc >/dev/null")
            .output()?;

        if !output.status.success() {
            anyhow::bail!("Failed to add ngrok GPG key");
        }

        let output = std::process::Command::new("sh")
            .arg("-c")
            .arg("echo 'deb https://ngrok-agent.s3.amazonaws.com buster main' | sudo tee /etc/apt/sources.list.d/ngrok.list")
            .output()?;

        if !output.status.success() {
            anyhow::bail!("Failed to add ngrok repository");
        }
    } else {
        ctx.log.dry_run(component, "Add ngrok repository");
    }

    ctx.execute(
        component,
        std::process::Command::new("sudo")
            .arg("apt")
            .arg("update"),
    )?;

    ctx.log.ok(component, "Installing ngrok");

    ctx.execute(
        component,
        std::process::Command::new("sudo")
            .arg("apt")
            .arg("install")
            .arg("-y")
            .arg("ngrok"),
    )?;

    ctx.log.ok(component, "ngrok installed successfully");
    ctx.log.warn(component, "Run 'ngrok config add-authtoken <token>' to configure");

    Ok(())
}

/// Detect rm guard
pub fn detect_rm_guard(_ctx: &SetupContext) -> Result<InstallState> {
    // Check if the rm function is defined in .bashrc
    let home = std::env::var("HOME")?;
    let bashrc_path = format!("{}/.bashrc", home);

    if let Ok(content) = std::fs::read_to_string(&bashrc_path) {
        if content.contains("rm() {") && content.contains("PREVIEW_DEPTH") {
            Ok(InstallState::Installed {
                version: None,
                details: vec!["rm guard function present in .bashrc".to_string()],
            })
        } else {
            Ok(InstallState::NotInstalled)
        }
    } else {
        Ok(InstallState::NotInstalled)
    }
}

/// Install rm guard
pub fn install_rm_guard(ctx: &SetupContext) -> Result<()> {
    let component = "rm_guard";

    let home = std::env::var("HOME")?;
    let bashrc_path = format!("{}/.bashrc", home);

    ctx.log.ok(component, "Installing rm guard function");

    let rm_guard_script = r#"
export PREVIEW_DEPTH=2
print_subfiles() {
    local dir=$1
    local prefix=$2
    local depth=$3
    local max_depth="${PREVIEW_DEPTH:-3}"

    # Colors
    local CYAN="\e[36m"
    local YELLOW="\e[33m"
    local RESET="\e[0m"

    if (( depth <= max_depth )); then
        ((depth++))
        local entries=("$dir"/*)
        local count=${#entries[@]}
        local i=0

        for entry in "${entries[@]}"; do
            ((i++))
            local base=$(basename "$entry")
            local connector="├──"
            local new_prefix="${prefix}│   "
            if (( i == count )); then
                connector="└──"
                new_prefix="${prefix}    "
            fi

            if [[ -d "$entry" ]]; then
                echo -e "${prefix}${CYAN}${connector}${RESET} $base/"
                print_subfiles "$entry" "$new_prefix" "$depth"
            else
                echo -e "${prefix}${YELLOW}${connector}${RESET} $base"
            fi
        done
    else
        echo -e "${prefix}..."
    fi
}

rm() {
    local CYAN="\e[36m"
    local BOLD="\e[1m"
    local RESET="\e[0m"

    echo -e "${CYAN}🗑️  ${BOLD}You are about to delete:${RESET}"

    to_delete=()
    for arg in "$@"; do
        if [[ "$arg" != -* ]]; then
            to_delete+=("$arg")
        fi
    done

    local disallowed=("/" "/mnt" "$HOME")
    for path in "${to_delete[@]}"; do
        local resolved=$(realpath "$path" 2>/dev/null)
        for dangerous in "${disallowed[@]}"; do
            if [[ "$resolved" == "$dangerous" ]]; then
                echo -e "\e[1;31m[ERROR]\e[0m Refusing to delete protected path: $resolved"
                return 1
            fi
        done
    done

    for path in "${to_delete[@]}"; do
        if [ -d "$path" ]; then
            echo -e "${CYAN}$(realpath "$path")/${RESET}"
            print_subfiles "$path" "    " 0
        else
            echo -e "⚠️  $(realpath -e "$path" 2>/dev/null || echo "[missing] $path")"
        fi
    done

    echo -ne "\n${BOLD}Confirm deletion? [y/N]: ${RESET}"
    read -r answer
    if [[ "$answer" =~ ^[Yy]$ ]]; then
        command rm "$@"
    else
        echo -e "${CYAN}❌ Deletion cancelled.${RESET}"
    fi
}
"#;

    if ctx.dry_run {
        ctx.log.dry_run(component, "Append rm guard function to ~/.bashrc");
        return Ok(());
    }

    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&bashrc_path)?;

    file.write_all(rm_guard_script.as_bytes())?;

    ctx.log.ok(component, "rm guard function installed successfully");
    ctx.log.warn(component, "Run 'source ~/.bashrc' or restart your shell to enable");

    Ok(())
}
