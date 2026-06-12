use super::component::InstallState;
use super::context::SetupContext;
use anyhow::{Context, Result};
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

/// Detect zoxide
pub fn detect_zoxide(ctx: &SetupContext) -> Result<InstallState> {
    if let Some(version) = command_version(ctx, "zoxide", &["--version"])? {
        Ok(InstallState::Installed {
            version: Some(version),
            details: vec![],
        })
    } else {
        Ok(InstallState::NotInstalled)
    }
}

/// Install zoxide
pub fn install_zoxide(ctx: &SetupContext) -> Result<()> {
    let component = "zoxide";

    require_command(ctx, component, "cargo")?;

    ctx.log.ok(component, "Installing zoxide via cargo");
    ctx.execute(
        component,
        Command::new("cargo").arg("install").arg("zoxide"),
    )?;

    log_applied(ctx, component, "zoxide installed successfully");
    if !ctx.dry_run {
        ctx.log.warn(
            component,
            "Add 'eval \"$(zoxide init --cmd cd bash)\"' to your ~/.bashrc to enable",
        );
    }

    Ok(())
}

/// Detect atuin
pub fn detect_atuin(ctx: &SetupContext) -> Result<InstallState> {
    if let Some(version) = command_version(ctx, "atuin", &["--version"])? {
        Ok(InstallState::Installed {
            version: Some(version),
            details: vec![],
        })
    } else {
        Ok(InstallState::NotInstalled)
    }
}

/// Install atuin
pub fn install_atuin(ctx: &SetupContext) -> Result<()> {
    let component = "atuin";

    require_command(ctx, component, "curl")?;

    ctx.log.ok(component, "Installing atuin");
    execute_shell(
        ctx,
        component,
        "curl --proto '=https' --tlsv1.2 -LsSf https://setup.atuin.sh | sh",
    )?;

    log_applied(ctx, component, "atuin installed successfully");

    Ok(())
}

/// Detect ngrok
pub fn detect_ngrok(ctx: &SetupContext) -> Result<InstallState> {
    if let Some(version) = command_version(ctx, "ngrok", &["version"])? {
        Ok(InstallState::Installed {
            version: Some(version),
            details: vec![],
        })
    } else {
        Ok(InstallState::NotInstalled)
    }
}

/// Install ngrok
pub fn install_ngrok(ctx: &SetupContext) -> Result<()> {
    let component = "ngrok";

    require_command(ctx, component, "curl")?;

    ctx.log.ok(component, "Adding ngrok repository");

    execute_shell(
        ctx,
        component,
        "curl -s https://ngrok-agent.s3.amazonaws.com/ngrok.asc | sudo tee /etc/apt/trusted.gpg.d/ngrok.asc >/dev/null",
    )?;
    execute_shell(
        ctx,
        component,
        "echo 'deb https://ngrok-agent.s3.amazonaws.com buster main' | sudo tee /etc/apt/sources.list.d/ngrok.list",
    )?;

    ctx.execute(
        component,
        std::process::Command::new("sudo").arg("apt").arg("update"),
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

    log_applied(ctx, component, "ngrok installed successfully");
    if !ctx.dry_run {
        ctx.log.warn(
            component,
            "Run 'ngrok config add-authtoken <token>' to configure",
        );
    }

    Ok(())
}

/// Detect rm guard
pub fn detect_rm_guard(_ctx: &SetupContext) -> Result<InstallState> {
    // Check if the rm function is defined in .bashrc
    let home = user_home()?;
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

    let home = user_home()?;
    let bashrc_path = format!("{}/.bashrc", home);

    if matches!(detect_rm_guard(ctx)?, InstallState::Installed { .. }) {
        ctx.log.ok(component, "rm guard already installed");
        return Ok(());
    }

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
        ctx.log
            .dry_run(component, "Append rm guard function to ~/.bashrc");
        return Ok(());
    }

    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&bashrc_path)?;

    file.write_all(rm_guard_script.as_bytes())?;

    log_applied(ctx, component, "rm guard function installed successfully");
    if !ctx.dry_run {
        ctx.log.warn(
            component,
            "Run 'source ~/.bashrc' or restart your shell to enable",
        );
    }

    Ok(())
}
