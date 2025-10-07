use anyhow::Result;
use camino::Utf8Path;

use super::write_template;

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

    write_template(destination, template)?;
    println!("  created {}", destination);
    Ok(())
}
