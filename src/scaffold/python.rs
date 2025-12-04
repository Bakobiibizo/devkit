use anyhow::Result;
use camino::Utf8Path;
use std::fs;

use super::write_template;

const RUFF: &str = "ruff.toml";
const MYPY: &str = "mypy.ini";
const PRECOMMIT: &str = ".pre-commit-config.yaml";
const CI_WORKFLOW: &str = ".github/workflows/ci.yml";

pub fn install() -> Result<()> {
    ensure_file(RUFF, "python/ruff.toml")?;
    ensure_file(MYPY, "python/mypy.ini")?;
    ensure_file(PRECOMMIT, "python/pre-commit-config.yaml")?;
    ensure_ci_workflow()?;

    println!("Python scaffolding complete");
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

fn ensure_file(target: &str, template: &str) -> Result<()> {
    let destination = Utf8Path::new(target);
    if destination.exists() {
        return Ok(());
    }

    write_template(destination, template)?;
    println!("  created {}", destination);
    Ok(())
}
