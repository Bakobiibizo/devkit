pub mod python;
pub mod rust;
pub mod typescript;

use crate::templates;
use anyhow::{Result, bail};
use camino::Utf8Path;

pub fn install(language: &str) -> Result<()> {
    match language {
        "rust" => rust::install(),
        "python" => python::install(),
        "typescript" | "ts" => typescript::install(),
        other => bail!("unsupported language scaffold: {other}"),
    }
}

pub fn write_template(destination: &Utf8Path, template: &str) -> Result<()> {
    templates::write_template(destination, template)
}
