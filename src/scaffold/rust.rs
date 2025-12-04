use anyhow::Result;
use camino::Utf8Path;
use std::fs;

use super::write_template;

const CARGO_CONFIG: &str = ".cargo/config.toml";
const DENY_FILE: &str = "deny.toml";
const CI_WORKFLOW: &str = ".github/workflows/ci.yml";

pub fn install() -> Result<()> {
    ensure_file(CARGO_CONFIG, "rust/cargo-config.toml")?;
    ensure_file(DENY_FILE, "rust/deny.toml")?;
    ensure_ci_workflow()?;

    println!("Rust scaffolding complete");
    Ok(())
}

fn ensure_ci_workflow() -> Result<()> {
    let destination = Utf8Path::new(CI_WORKFLOW);
    if destination.exists() {
        return Ok(());
    }

    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }

    write_template(destination, "rust/.github/workflows/ci.yml")?;
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
