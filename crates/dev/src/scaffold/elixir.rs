use anyhow::Result;
use camino::Utf8Path;
use std::fs;

use super::write_template;

const FORMATTER: &str = ".formatter.exs";
const CI_WORKFLOW: &str = ".github/workflows/ci.yml";

pub fn install(force: bool) -> Result<()> {
    ensure_file(FORMATTER, "elixir/.formatter.exs", force)?;
    ensure_ci_workflow(force)?;

    println!("Elixir scaffolding complete");
    Ok(())
}

fn ensure_ci_workflow(force: bool) -> Result<()> {
    let destination = Utf8Path::new(CI_WORKFLOW);
    if destination.exists() {
        if force {
            write_template(destination, "elixir/.github/workflows/ci.yml")?;
            println!("  overwritten {}", destination);
        } else {
            println!("  skipped {}", destination);
        }
        return Ok(());
    }

    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }

    write_template(destination, "elixir/.github/workflows/ci.yml")?;
    println!("  created {}", destination);
    Ok(())
}

fn ensure_file(target: &str, template: &str, force: bool) -> Result<()> {
    let destination = Utf8Path::new(target);
    if destination.exists() {
        if force {
            write_template(destination, template)?;
            println!("  overwritten {}", destination);
        } else {
            println!("  skipped {}", destination);
        }
        return Ok(());
    }

    write_template(destination, template)?;
    println!("  created {}", destination);
    Ok(())
}
