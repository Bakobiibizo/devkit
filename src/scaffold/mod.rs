pub mod python;
pub mod rust;
pub mod typescript;

use std::fs;

use anyhow::{Context, Result, anyhow, bail};
use camino::Utf8Path;
use rust_embed::RustEmbed;

pub fn install(language: &str) -> Result<()> {
    match language {
        "rust" => rust::install(),
        "python" => python::install(),
        "typescript" | "ts" => typescript::install(),
        other => bail!("unsupported language scaffold: {other}"),
    }
}

#[derive(RustEmbed)]
#[folder = "templates"]
struct Templates;

pub fn write_template(destination: &Utf8Path, template: &str) -> Result<()> {
    let file = Templates::get(template)
        .ok_or_else(|| anyhow!("embedded template `{}` missing", template))?;
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).with_context(|| format!("creating directory {}", parent))?;
    }
    fs::write(destination, file.data.as_ref()).with_context(|| format!("writing {}", destination))
}
