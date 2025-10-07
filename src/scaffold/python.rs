use anyhow::Result;
use camino::Utf8Path;

use super::write_template;

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

    write_template(destination, template)?;
    println!("  created {}", destination);
    Ok(())
}
