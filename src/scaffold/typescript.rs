use std::fs;

use anyhow::{Context, Result};
use camino::Utf8Path;

use super::template_path;

const ESLINT: &str = "eslint.config.ts";
const TSCONFIG: &str = "tsconfig.json";
const VITEST: &str = "vitest.config.ts";
const PRETTIER: &str = ".prettierrc.json";

pub fn install() -> Result<()> {
    ensure_file(ESLINT, "typescript/eslint.config.ts")?;
    ensure_file(TSCONFIG, "typescript/tsconfig.json")?;
    ensure_file(VITEST, "typescript/vitest.config.ts")?;
    ensure_file(PRETTIER, "typescript/prettierrc.json")?;

    println!("TypeScript scaffolding complete");
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
