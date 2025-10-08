use std::{fs, io::Write, process::Command};

use anyhow::{Context, Result, anyhow, bail};
use camino::{Utf8Path, Utf8PathBuf};
use chrono::Utc;
use semver::{Prerelease, Version};
use toml_edit::{DocumentMut, value};

use crate::{
    cli::{ChangelogArgs, VersionBump, VersionCommand},
    config::DevConfig,
};

pub fn handle(config: &DevConfig, dry_run: bool, command: VersionCommand) -> Result<()> {
    match command {
        VersionCommand::Show => show_version(config),
        VersionCommand::Bump(args) => bump_version(config, &args, dry_run),
        VersionCommand::Changelog(args) => print_changelog(config, &args),
    }
}

fn show_version(config: &DevConfig) -> Result<()> {
    let (path, kind) = locate_version_file(config)?;
    let doc = read_manifest(&path, kind)?;
    let version = current_version(&doc, kind)?;
    println!("{}", version);
    Ok(())
}

fn bump_version(config: &DevConfig, args: &VersionBump, dry_run: bool) -> Result<()> {
    let (path, kind) = locate_version_file(config)?;
    let mut doc = read_manifest(&path, kind)?;
    let current = current_version(&doc, kind)?;

    let target = if let Some(custom) = &args.custom {
        Version::parse(custom).with_context(|| format!("parsing custom version `{}`", custom))?
    } else {
        increment_version(&current, args.level)?
    };

    if dry_run {
        println!(
            "[dry-run] would update {} from {} to {}",
            path, current, target
        );
    } else {
        write_version(&mut doc, kind, &target);
        fs::write(&path, doc.to_string()).with_context(|| format!("writing {}", path))?;
        println!("Updated {} to {}", path, target);
    }

    let mut staged_paths = vec![path.clone()];

    if !args.no_changelog
        && let Some(changelog) = changelog_path(config)?
    {
        update_changelog(&changelog, &target, dry_run)?;
        staged_paths.push(changelog);
    }

    if !args.no_commit {
        git_add(&staged_paths, dry_run)?;
        let message = format!("chore: release {}", target);
        git_commit(&message, dry_run)?;
    }

    if args.tag {
        let tag_name = format!("v{}", target);
        git_tag(&tag_name, dry_run)?;
    }

    Ok(())
}

fn print_changelog(_config: &DevConfig, args: &ChangelogArgs) -> Result<()> {
    let range = if let Some(since) = &args.since {
        format!("{}..HEAD", since)
    } else if args.unreleased {
        let tag = latest_tag()?.unwrap_or_else(|| "HEAD^".to_string());
        format!("{}..HEAD", tag)
    } else {
        format!("{}..HEAD", DEFAULT_BASE_BRANCH)
    };

    let commits = collect_commits(&range)?;
    if commits.is_empty() {
        println!("No commits for range {}", range);
    } else {
        println!("Changelog for {}:", range);
        for commit in commits {
            println!("- {}", commit);
        }
    }
    Ok(())
}

fn read_manifest(path: &Utf8Path, kind: VersionFileKind) -> Result<DocumentMut> {
    match kind {
        VersionFileKind::CargoToml => {
            let contents = fs::read_to_string(path).with_context(|| format!("reading {}", path))?;
            contents
                .parse::<DocumentMut>()
                .with_context(|| format!("parsing {}", path))
        }
    }
}

fn current_version(doc: &DocumentMut, kind: VersionFileKind) -> Result<Version> {
    match kind {
        VersionFileKind::CargoToml => doc["package"]["version"]
            .as_str()
            .ok_or_else(|| anyhow!("missing package.version in Cargo.toml"))
            .and_then(|s| Version::parse(s).with_context(|| format!("parsing version `{}`", s))),
    }
}

fn write_version(doc: &mut DocumentMut, kind: VersionFileKind, version: &Version) {
    match kind {
        VersionFileKind::CargoToml => doc["package"]["version"] = value(version.to_string()),
    }
}

fn increment_version(version: &Version, level: crate::cli::BumpLevel) -> Result<Version> {
    let new_version = match level {
        crate::cli::BumpLevel::Major => Version::new(version.major + 1, 0, 0),
        crate::cli::BumpLevel::Minor => Version::new(version.major, version.minor + 1, 0),
        crate::cli::BumpLevel::Patch => {
            Version::new(version.major, version.minor, version.patch + 1)
        }
        crate::cli::BumpLevel::Prerelease => bump_prerelease(version)?,
    };
    Ok(new_version)
}

fn bump_prerelease(version: &Version) -> Result<Version> {
    let mut new = version.clone();
    if new.pre.is_empty() {
        new.pre = Prerelease::new("alpha.1")?;
    } else {
        let mut segments: Vec<String> =
            new.pre.as_str().split('.').map(|s| s.to_string()).collect();
        if let Some(last) = segments.last_mut() {
            if let Ok(num) = last.parse::<u64>() {
                *last = (num + 1).to_string();
            } else {
                segments.push("1".into());
            }
        } else {
            segments.push("alpha".into());
            segments.push("1".into());
        }
        new.pre = Prerelease::new(&segments.join("."))?;
    }
    Ok(new)
}

fn locate_version_file(config: &DevConfig) -> Result<(Utf8PathBuf, VersionFileKind)> {
    let cwd = std::env::current_dir().context("determining current directory")?;
    let cwd = Utf8PathBuf::from_path_buf(cwd)
        .map_err(|_| anyhow!("current directory is not valid UTF-8"))?;

    if let Some(raw) = config
        .git
        .as_ref()
        .and_then(|git| git.version_file.as_deref())
    {
        let path = resolve_path(&cwd, raw)?;
        let kind = detect_version_file(&path)?;
        return Ok((path, kind));
    }

    let default = cwd.join("Cargo.toml");
    Ok((default, VersionFileKind::CargoToml))
}

fn detect_version_file(path: &Utf8Path) -> Result<VersionFileKind> {
    match path.file_name() {
        Some("Cargo.toml") => Ok(VersionFileKind::CargoToml),
        Some(name) => bail!("unsupported version file `{}`", name),
        None => bail!("version file must not be a directory"),
    }
}

fn update_changelog(path: &Utf8Path, version: &Version, dry_run: bool) -> Result<()> {
    let date = Utc::now().format("%Y-%m-%d");
    let mut section = format!("## {} - v{}\n\n", date, version);
    section.push_str("- Describe the notable changes here.\n\n");

    if dry_run {
        println!("[dry-run] update {} with:\n{}", path, section);
        return Ok(());
    }

    let mut content = if path.exists() {
        fs::read_to_string(path).with_context(|| format!("reading {}", path))?
    } else {
        String::from("# Changelog\n\n## Unreleased\n\n")
    };

    if let Some(idx) = content.find("## Unreleased") {
        let insert_at = content[idx..]
            .find('\n')
            .map(|offset| idx + offset + 1)
            .unwrap_or(content.len());
        content.insert_str(insert_at, &format!("\n{}", section));
    } else {
        if !content.ends_with('\n') {
            content.push('\n');
        }
        content.push_str(&section);
    }

    let mut file = fs::File::create(path).with_context(|| format!("opening {}", path))?;
    file.write_all(content.as_bytes())
        .with_context(|| format!("writing {}", path))?;
    Ok(())
}

fn changelog_path(config: &DevConfig) -> Result<Option<Utf8PathBuf>> {
    let cwd = Utf8PathBuf::from_path_buf(
        std::env::current_dir().context("determining current directory")?,
    )
    .map_err(|_| anyhow!("current directory is not valid UTF-8"))?;

    let path = if let Some(path) = config.git.as_ref().and_then(|git| git.changelog.as_deref()) {
        resolve_path(&cwd, path)?
    } else {
        cwd.join("CHANGELOG.md")
    };

    Ok(Some(path))
}

fn resolve_path(base: &Utf8Path, raw: &str) -> Result<Utf8PathBuf> {
    let path = Utf8PathBuf::from(raw);
    if path.is_absolute() {
        Ok(path)
    } else {
        Ok(base.join(path))
    }
}

fn git_add(paths: &[Utf8PathBuf], dry_run: bool) -> Result<()> {
    if paths.is_empty() {
        return Ok(());
    }
    let mut args = vec!["add".into()];
    args.extend(paths.iter().map(|p| p.to_string()));
    run_git(&args, dry_run)
}

fn git_commit(message: &str, dry_run: bool) -> Result<()> {
    run_git(&["commit".into(), "-m".into(), message.into()], dry_run)
}

fn git_tag(tag: &str, dry_run: bool) -> Result<()> {
    run_git(&["tag".into(), tag.into()], dry_run)
}

fn run_git(args: &[String], dry_run: bool) -> Result<()> {
    if dry_run {
        let display = format!("git {}", args.join(" "));
        println!("[dry-run] {}", display);
        return Ok(());
    }

    let status = Command::new("git")
        .args(args)
        .status()
        .with_context(|| format!("running git {}", args.join(" ")))?;
    if !status.success() {
        let code = status.code().unwrap_or(-1);
        bail!("git command failed with status {}", code);
    }
    Ok(())
}

fn collect_commits(range: &str) -> Result<Vec<String>> {
    let output = Command::new("git")
        .args(["log", range, "--pretty=format:%s"])
        .output()
        .with_context(|| format!("collecting commits for {}", range))?;
    if !output.status.success() {
        let code = output.status.code().unwrap_or(-1);
        bail!("git log failed with status {}", code);
    }
    let commits = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|line| line.trim().to_owned())
        .filter(|line| !line.is_empty())
        .collect();
    Ok(commits)
}

fn latest_tag() -> Result<Option<String>> {
    let output = Command::new("git")
        .args(["describe", "--tags", "--abbrev=0"])
        .output();
    match output {
        Ok(out) if out.status.success() => Ok(Some(
            String::from_utf8_lossy(&out.stdout).trim().to_string(),
        )),
        _ => Ok(None),
    }
}

#[derive(Clone, Copy)]
enum VersionFileKind {
    CargoToml,
}

const DEFAULT_BASE_BRANCH: &str = "release-candidate";
