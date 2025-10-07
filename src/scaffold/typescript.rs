use anyhow::Result;
use camino::Utf8Path;

use super::write_template;

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

    write_template(destination, template)?;
    println!("  created {}", destination);
    Ok(())
}
