use std::fs;

use anyhow::{Context, Result, anyhow};
use camino::Utf8Path;
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "templates"]
struct Templates;

pub fn get_bytes(path: &str) -> Result<Vec<u8>> {
    let file = Templates::get(path).ok_or_else(|| anyhow!("embedded template `{}` missing", path))?;
    Ok(file.data.as_ref().to_vec())
}

pub fn get_string(path: &str) -> Result<String> {
    let bytes = get_bytes(path)?;
    std::str::from_utf8(&bytes)
        .with_context(|| format!("decoding embedded template `{}`", path))
        .map(|value| value.to_owned())
}

pub fn write_to(destination: &Utf8Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("creating directory {}", parent))?;
    }
    fs::write(destination, bytes).with_context(|| format!("writing {}", destination))
}

pub fn write_template(destination: &Utf8Path, template: &str) -> Result<()> {
    let bytes = get_bytes(template)?;
    write_to(destination, &bytes)
}
