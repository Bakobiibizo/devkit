pub mod elixir;
pub mod python;
pub mod rust;
pub mod typescript;

use crate::templates;
use anyhow::{Result, bail};
use camino::Utf8Path;

pub fn install(language: &str, force: bool) -> Result<()> {
    match language {
        "elixir" | "ex" => elixir::install(force),
        "rust" => rust::install(force),
        "python" => python::install(force),
        "typescript" | "ts" => typescript::install(force),
        other => bail!("unsupported language scaffold: {other}"),
    }
}

pub fn write_template(destination: &Utf8Path, template: &str) -> Result<()> {
    templates::write_template(destination, template)
}
