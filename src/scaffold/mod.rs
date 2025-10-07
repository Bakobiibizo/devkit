pub mod python;
pub mod rust;
pub mod typescript;

use anyhow::{bail, Result};

pub fn install(language: &str) -> Result<()> {
    match language {
        "rust" => rust::install(),
        "python" => python::install(),
        "typescript" | "ts" => typescript::install(),
        other => bail!("unsupported language scaffold: {other}"),
    }
}
