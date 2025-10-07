use std::fs;

use anyhow::{Context, Result};
use camino::Utf8Path;

use super::template_path;

const RUFF: &str = "ruff.toml";
const MYPY: &str = "mypy.ini";
const PRECOMMIT: &str = ".pre-commit-config.yaml";

pub fn install() -> Result<()> {
    ensure_file(RUFF, "python/ruff.toml")?;
    ensure_file(MYPY, "python/mypy.ini")?;
    ensure_file(PRECOMMIT, "python/pre-commit-config.yaml")?;

    println!("Python scaffolding complete");
    Ok(())
}

fn ensure_file(target: &str, template: &str) -> Result<()> {
    let destination = Utf8Path::new(target);
    if destination.exists() {
        return Ok(());
    }

    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).with_context(|| format!("creating directory {}", parent))?;
    }

    let template_path = template_path(template)?;
    let contents = fs::read_to_string(&template_path)
        .with_context(|| format!("reading template {}", template_path))?;
    fs::write(destination, contents).with_context(|| format!("writing {}", destination))?;
    println!("  created {}", destination);
    Ok(())
}
