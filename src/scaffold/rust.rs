use std::fs;

use anyhow::{Context, Result};
use camino::Utf8Path;

use super::template_path;

const CARGO_CONFIG: &str = ".cargo/config.toml";
const DENY_FILE: &str = "deny.toml";

pub fn install() -> Result<()> {
    ensure_file(CARGO_CONFIG, "rust/cargo-config.toml")?;
    ensure_file(DENY_FILE, "rust/deny.toml")?;

    println!("Rust scaffolding complete");
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
