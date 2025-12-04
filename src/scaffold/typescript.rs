use anyhow::Result;
use camino::Utf8Path;
use std::fs;

use super::write_template;

const ESLINT: &str = "eslint.config.ts";
const TSCONFIG: &str = "tsconfig.json";
const VITEST: &str = "vitest.config.ts";
const PRETTIER: &str = ".prettierrc.json";
const CI_WORKFLOW: &str = ".github/workflows/ci.yml";

pub fn install() -> Result<()> {
    ensure_file(ESLINT, "typescript/eslint.config.ts")?;
    ensure_file(TSCONFIG, "typescript/tsconfig.json")?;
    ensure_file(VITEST, "typescript/vitest.config.ts")?;
    ensure_file(PRETTIER, "typescript/prettierrc.json")?;
    ensure_ci_workflow()?;

    println!("TypeScript scaffolding complete");
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

    write_template(destination, "typescript/.github/workflows/ci.yml")?;
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
