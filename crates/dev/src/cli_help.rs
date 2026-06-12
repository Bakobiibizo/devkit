use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::config;

pub(crate) fn should_append_dynamic_help(args: &[std::ffi::OsString]) -> bool {
    let mut saw_help = false;
    let mut positional = Vec::new();
    let mut skip_next = false;

    for arg in args.iter().skip(1) {
        if skip_next {
            skip_next = false;
            continue;
        }

        let value = arg.to_string_lossy();
        match value.as_ref() {
            "--help" | "-h" => {
                saw_help = true;
                continue;
            }
            "--file" | "-f" | "--chdir" | "-C" | "--project" | "--language" | "-l" => {
                skip_next = true;
                continue;
            }
            _ => {}
        }

        if value.starts_with("--file=")
            || value.starts_with("--chdir=")
            || value.starts_with("--project=")
            || value.starts_with("--language=")
            || value.starts_with('-')
        {
            continue;
        }

        positional.push(value.into_owned());
    }

    saw_help && positional.is_empty()
}

pub(crate) fn dynamic_help(args: &[std::ffi::OsString]) -> Result<Option<String>> {
    let Some(path) = resolve_help_config_path(args) else {
        return Ok(None);
    };
    if !path.exists() {
        return Ok(None);
    }

    let path = camino::Utf8PathBuf::from_path_buf(path)
        .map_err(|_| anyhow::anyhow!("config path must be valid UTF-8"))?;
    let cfg = match config::load_from_path(&path) {
        Ok(cfg) => cfg,
        Err(err) => {
            return Ok(Some(format!(
                "\nConfigured items unavailable\n  Could not load config {path}: {err:#}\n"
            )));
        }
    };
    let mut out = String::new();
    let task_count = cfg.tasks.as_ref().map(|tasks| tasks.len()).unwrap_or(0);
    let language_summary = summarize_languages(&cfg);

    if task_count == 0 && language_summary.is_empty() {
        return Ok(None);
    }

    out.push_str("\nConfigured workflows\n");
    out.push_str("  verbs: dev fmt|lint|type|test|fix|check|ci\n");
    if task_count > 0 {
        out.push_str(&format!(
            "  tasks: {task_count} configured (run `dev list` for names)\n"
        ));
        if let Some(example) = first_task_name(&cfg) {
            out.push_str(&format!("  example: dev run {example}\n"));
        }
    }
    if !language_summary.is_empty() {
        out.push_str(&format!("  languages: {}\n", language_summary.join("; ")));
    }
    out.push_str("  details: dev list | dev config show\n");
    out.push_str(&format!("\nConfig source: {path}\n"));
    Ok(Some(out))
}

fn resolve_help_config_path(args: &[std::ffi::OsString]) -> Option<PathBuf> {
    let mut iter = args.iter().peekable();
    while let Some(arg) = iter.next() {
        let value = arg.to_string_lossy();
        if value == "--file" || value == "-f" {
            return iter.next().map(PathBuf::from);
        }
        if let Some(path) = value.strip_prefix("--file=") {
            return Some(PathBuf::from(path));
        }
    }

    let cwd = std::env::current_dir().ok()?;
    for dir in cwd.ancestors() {
        for candidate in config_candidates(dir) {
            if candidate.exists() {
                return Some(candidate);
            }
        }
    }

    dirs::home_dir().map(|home| home.join(".dev").join("config.toml"))
}

fn first_task_name(cfg: &config::DevConfig) -> Option<&str> {
    cfg.tasks
        .as_ref()
        .and_then(|tasks| tasks.keys().next().map(String::as_str))
}

fn summarize_languages(cfg: &config::DevConfig) -> Vec<String> {
    cfg.languages
        .as_ref()
        .map(|languages| {
            languages
                .iter()
                .map(|(name, language)| {
                    let verbs = language
                        .pipelines
                        .as_ref()
                        .map(pipeline_names)
                        .unwrap_or_default();
                    if verbs.is_empty() {
                        format!("{name} (no pipelines)")
                    } else {
                        format!("{name} ({})", verbs.join(","))
                    }
                })
                .collect()
        })
        .unwrap_or_default()
}

fn pipeline_names(pipelines: &config::Pipelines) -> Vec<&'static str> {
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

fn config_candidates(dir: &Path) -> [PathBuf; 2] {
    [
        dir.join(".dev").join("config.toml"),
        dir.join("tools").join("dev").join("config.toml"),
    ]
}
