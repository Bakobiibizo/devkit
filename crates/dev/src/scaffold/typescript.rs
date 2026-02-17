use anyhow::Result;
use camino::Utf8Path;
use std::fs;

use super::write_template;

const ESLINT: &str = "eslint.config.ts";
const TSCONFIG: &str = "tsconfig.json";
const VITEST: &str = "vitest.config.ts";
const PRETTIER: &str = ".prettierrc.json";
const CI_WORKFLOW: &str = ".github/workflows/ci.yml";
const GITIGNORE: &str = ".gitignore";

pub fn install(force: bool) -> Result<()> {
    ensure_file(GITIGNORE, "typescript/.gitignore", force)?;
    ensure_file(ESLINT, "typescript/eslint.config.ts", force)?;
    ensure_file(TSCONFIG, "typescript/tsconfig.json", force)?;
    ensure_file(VITEST, "typescript/vitest.config.ts", force)?;
    ensure_file(PRETTIER, "typescript/prettierrc.json", force)?;
    ensure_ci_workflow(force)?;

    println!("TypeScript scaffolding complete");
    Ok(())
}

fn ensure_ci_workflow(force: bool) -> Result<()> {
    let destination = Utf8Path::new(CI_WORKFLOW);
    if destination.exists() {
        if force {
            write_template(destination, "typescript/.github/workflows/ci.yml")?;
            println!("  overwritten {}", destination);
        } else {
            println!("  skipped {}", destination);
        }
        return Ok(());
    }

    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }

    write_template(destination, "typescript/.github/workflows/ci.yml")?;
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
