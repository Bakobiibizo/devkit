use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};
use camino::Utf8Path;
use std::fs;

use super::write_template;

const PYPROJECT: &str = "pyproject.toml";
const CI_WORKFLOW: &str = ".github/workflows/ci.yml";
const GITIGNORE: &str = ".gitignore";

pub fn install() -> Result<()> {
    ensure_uv()?; // installs uv if necessary
    ensure_uv_tool("ruff")?;
    ensure_uv_tool("mypy")?;
    ensure_file(GITIGNORE, "python/.gitignore")?;
    ensure_file(PYPROJECT, "python/pyproject.toml")?;
    ensure_ci_workflow()?;

    println!("Python scaffolding complete");
    Ok(())
}

fn ensure_file(target: &str, template: &str) -> Result<()> {
    let destination = Utf8Path::new(target);
    if destination.exists() {
        return Ok(());
    }

    write_template(destination, template)?;
    println!("  created {}", destination);
    Ok(())
}

fn ensure_ci_workflow() -> Result<()> {
    let destination = Utf8Path::new(CI_WORKFLOW);
    if destination.exists() {
        return Ok(());
    }

    // Ensure .github/workflows directory exists
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }

    write_template(destination, "python/.github/workflows/ci.yml")?;
    println!("  created {}", destination);
    Ok(())
}

fn ensure_uv() -> Result<()> {
    if Command::new("uv")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
    {
        return Ok(());
    }

    println!("Installing `uv` via pip...");
    let status = Command::new("python3")
        .args(["-m", "pip", "install", "--user", "uv"])
        .status()
        .or_else(|_| {
            Command::new("python")
                .args(["-m", "pip", "install", "--user", "uv"])
                .status()
        })
        .context("installing uv with pip")?;

    if !status.success() {
        bail!(
            "failed to install uv via pip (exit code {:?})",
            status.code()
        );
    }

    Ok(())
}

fn ensure_uv_tool(tool: &str) -> Result<()> {
    println!("Ensuring `{}` is available via `uv tool install`...", tool);
    let status = Command::new("uv")
        .args(["tool", "install", tool])
        .status()
        .with_context(|| format!("installing {} via uv", tool))?;
    if !status.success() {
        bail!(
            "uv tool install {} failed with exit code {:?}",
            tool,
            status.code()
        );
    }
    Ok(())
}
