use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;

use anyhow::{Context, Result, bail};
use camino::Utf8Path;
use serde::Deserialize;
use toml::Value;
use toml_edit::{DocumentMut, value};

use crate::scaffold;

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

pub fn write_example_config(path: &Utf8Path, overwrite: bool) -> Result<()> {
    if path.exists() && !overwrite {
        bail!(
            "{} already exists; rerun with --force to overwrite",
            path
        );
    }

    scaffold::write_template(path, "config/example.config.toml")
}

pub fn set_default_language(path: &Utf8Path, language: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("creating directory {}", parent))?;
    }

    let mut doc: DocumentMut = if path.exists() {
        let raw = fs::read_to_string(path).with_context(|| format!("reading config {}", path))?;
        raw.parse()
            .with_context(|| format!("parsing config {}", path))?
    } else {
        DocumentMut::new()
    };

    doc["default_language"] = value(language);

    fs::write(path, doc.to_string()).with_context(|| format!("writing config {}", path))
}

pub fn format_summary(config: &DevConfig) -> String {
    let mut out = String::new();
    let default_language = config.default_language.as_deref().unwrap_or("<none>");
    let task_count = config.tasks.as_ref().map(|t| t.len()).unwrap_or(0);
    let language_count = config.languages.as_ref().map(|l| l.len()).unwrap_or(0);

    let _ = writeln!(out, "Default language: {}", default_language);
    let _ = writeln!(out, "Tasks defined: {}", task_count);
    let _ = writeln!(out, "Languages configured: {}", language_count);

    if let Some(languages) = &config.languages {
        for (name, language) in languages {
            let installs = language.install.as_ref().map(|v| v.len()).unwrap_or(0);
            let pipelines = language
                .pipelines
                .as_ref()
                .map(collect_pipeline_names)
                .unwrap_or_default();
            let pipeline_display = if pipelines.is_empty() {
                "none".to_string()
            } else {
                pipelines.join(", ")
            };
            let _ = writeln!(
                out,
                "  - {} (pipelines: {}; install cmds: {})",
                name, pipeline_display, installs
            );
        }
    }

    out
}

fn collect_pipeline_names(pipelines: &Pipelines) -> Vec<&'static str> {
    let mut names = Vec::new();
    if pipelines.fmt.is_some() {
        names.push("fmt");
    }
    if pipelines.lint.is_some() {
        names.push("lint");
    }
    if pipelines.type_check.is_some() {
        names.push("type");
    }
    if pipelines.test.is_some() {
        names.push("test");
    }
    if pipelines.fix.is_some() {
        names.push("fix");
    }
    if pipelines.check.is_some() {
        names.push("check");
    }
    if pipelines.ci.is_some() {
        names.push("ci");
    }
    names
}
