use std::collections::BTreeMap;
use std::fs;

use anyhow::{Context, Result};
use camino::Utf8Path;
use serde::Deserialize;
use toml::Value;

/// Root configuration document loaded from `~/.dev/config.toml` by default.
#[derive(Debug, Deserialize)]
pub struct DevConfig {
    pub default_language: Option<String>,
    pub tasks: Option<BTreeMap<String, Task>>,
    pub languages: Option<BTreeMap<String, Language>>,
    pub git: Option<GitConfig>,
}

#[derive(Debug, Deserialize)]
pub struct Task {
    pub commands: Vec<Value>,
    #[serde(default)]
    pub allow_fail: bool,
}

#[derive(Debug, Deserialize)]
pub struct Language {
    pub install: Option<Vec<Vec<String>>>,
    pub pipelines: Option<Pipelines>,
}

#[derive(Debug, Deserialize, Default)]
pub struct Pipelines {
    pub fmt: Option<Vec<String>>,
    pub lint: Option<Vec<String>>,
    #[serde(rename = "type")]
    pub type_check: Option<Vec<String>>,
    pub test: Option<Vec<String>>,
    pub fix: Option<Vec<String>>,
    pub check: Option<Vec<String>>,
    pub ci: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
pub struct GitConfig {
    pub main_branch: Option<String>,
    pub release_branch: Option<String>,
    pub version_file: Option<String>,
    pub changelog: Option<String>,
}

/// Load a configuration file from disk and deserialize it.
pub fn load_from_path(path: &Utf8Path) -> Result<DevConfig> {
    let raw = fs::read_to_string(path).with_context(|| format!("reading config {}", path))?;
    toml::from_str(&raw).with_context(|| format!("parsing config {}", path))
}
