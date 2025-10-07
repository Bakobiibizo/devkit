pub mod python;
pub mod rust;
pub mod typescript;

use std::path::PathBuf;

use anyhow::{Context, Result, anyhow, bail};
use camino::Utf8PathBuf;

const TEMPLATE_DIR: &str = "templates";

pub fn install(language: &str) -> Result<()> {
    match language {
        "rust" => rust::install(),
        "python" => python::install(),
        "typescript" | "ts" => typescript::install(),
        other => bail!("unsupported language scaffold: {other}"),
    }
}

pub fn template_path(template: &str) -> Result<Utf8PathBuf> {
    let root = project_root()?;
    let candidate = root.join(TEMPLATE_DIR).join(template);
    Utf8PathBuf::from_path_buf(candidate).map_err(|_| anyhow!("template path not valid UTF-8"))
}

fn project_root() -> Result<PathBuf> {
    let mut dir = std::env::current_dir().context("determining current directory")?;
    loop {
        if dir.join("Cargo.toml").exists() {
            return Ok(dir);
        }
        if !dir.pop() {
            bail!("could not locate project root");
        }
    }
}
