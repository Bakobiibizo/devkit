use std::fs::{self, File};

use anyhow::{Context, Result, anyhow};
use camino::{Utf8Path, Utf8PathBuf};

const ENV_FILENAME: &str = ".env";

/// Lightweight representation of a `.env` file.
#[derive(Debug)]
pub struct EnvFile {
    path: Utf8PathBuf,
    lines: Vec<Line>,
}

impl EnvFile {
    pub fn load(path: &Utf8Path) -> Result<Self> {
        let contents = if path.exists() {
            fs::read_to_string(path).with_context(|| format!("reading {}", path))?
        } else {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)
                    .with_context(|| format!("creating directory {}", parent))?;
            }
            File::create(path).with_context(|| format!("creating {}", path))?;
            String::new()
        };

        let lines = parse_lines(&contents);

        Ok(Self {
            path: path.to_owned(),
            lines,
        })
    }

    pub fn path(&self) -> &Utf8Path {
        &self.path
    }

    pub fn entries(&self) -> impl Iterator<Item = (&str, &str)> {
        self.lines.iter().filter_map(|line| match line {
            Line::Entry { key, value } => Some((key.as_str(), value.as_str())),
            _ => None,
        })
    }

    pub fn upsert(&mut self, key: &str, value: &str) {
        for line in &mut self.lines {
            if let Line::Entry {
                key: existing,
                value: existing_value,
            } = line
            {
                if existing == key {
                    *existing_value = value.to_owned();
                    return;
                }
            }
        }

        self.lines.push(Line::Entry {
            key: key.to_owned(),
            value: value.to_owned(),
        });
    }

    pub fn remove(&mut self, key: &str) -> bool {
        let mut removed = false;
        self.lines.retain(|line| match line {
            Line::Entry { key: existing, .. } if existing == key => {
                removed = true;
                false
            }
            _ => true,
        });
        removed
    }

    pub fn save(&self) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).with_context(|| format!("creating directory {}", parent))?;
        }

        let mut buffer = String::new();
        for (idx, line) in self.lines.iter().enumerate() {
            if idx > 0 {
                buffer.push('\n');
            }
            match line {
                Line::Entry { key, value } => {
                    buffer.push_str(key);
                    buffer.push('=');
                    buffer.push_str(value);
                }
                Line::Comment(text) => buffer.push_str(text),
                Line::Blank => {}
            }
        }

        fs::write(&self.path, buffer).with_context(|| format!("writing {}", self.path))
    }
}

pub fn locate(start: &Utf8Path) -> Result<Utf8PathBuf> {
    let mut current: Option<&Utf8Path> = Some(start);
    while let Some(dir) = current {
        let candidate = dir.join(ENV_FILENAME);
        if candidate.exists() {
            return Ok(candidate);
        }
        current = dir.parent();
    }

    if let Some(git_root) = find_git_root(start) {
        let candidate = git_root.join(ENV_FILENAME);
        if candidate.exists() {
            return Ok(candidate);
        }
        return Ok(candidate);
    }

    Ok(start.join(ENV_FILENAME))
}

fn parse_lines(contents: &str) -> Vec<Line> {
    contents
        .lines()
        .map(|line| {
            let trimmed = line.trim_end_matches(['\r']);
            if trimmed.trim_start().starts_with('#') {
                Line::Comment(trimmed.to_owned())
            } else if trimmed.is_empty() {
                Line::Blank
            } else {
                let mut parts = trimmed.splitn(2, '=');
                let key = parts.next().unwrap_or_default().trim().to_owned();
                let value = parts.next().unwrap_or_default().to_owned();
                Line::Entry { key, value }
            }
        })
        .collect()
}

fn find_git_root(start: &Utf8Path) -> Option<Utf8PathBuf> {
    let mut current = Some(start);
    while let Some(dir) = current {
        if dir.join(".git").exists() {
            return Some(dir.to_owned());
        }
        current = dir.parent();
    }
    None
}

pub fn current_working_dir() -> Result<Utf8PathBuf> {
    let cwd = std::env::current_dir().context("determining current directory")?;
    Utf8PathBuf::from_path_buf(cwd).map_err(|_| anyhow!("current directory is not valid UTF-8"))
}

#[derive(Debug)]
enum Line {
    Entry { key: String, value: String },
    Comment(String),
    Blank,
}
