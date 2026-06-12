use std::fs;

use anyhow::{Context, Result, anyhow, bail};
use camino::{Utf8Path, Utf8PathBuf};
use chrono::Utc;
use semver::{Prerelease, Version};
use toml_edit::{DocumentMut, value};

use crate::{
    cli::{BumpLevel, ChangelogArgs, VersionBump, VersionCommand},
    config::DevConfig,
    core::{
        changelog::prepend_release_section,
        git::{collect_commit_subjects, latest_tag, run_git},
    },
};

/// Result of a version bump operation.
pub struct BumpResult {
    pub new_version: Version,
    pub changed_paths: Vec<Utf8PathBuf>,
}

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

/// Perform a version bump, updating version files on disk.
/// Does NOT update changelog or run git operations.
pub fn perform_bump(
    config: &DevConfig,
    level: BumpLevel,
    custom: Option<&str>,
    dry_run: bool,
) -> Result<BumpResult> {
    let cwd = std::env::current_dir().context("determining current directory")?;
    let cwd = Utf8PathBuf::from_path_buf(cwd)
        .map_err(|_| anyhow!("current directory is not valid UTF-8"))?;

    let version_files = if let Some(tauri_files) = locate_tauri_version_files(&cwd) {
        tauri_files
    } else {
        let (path, kind) = locate_version_file(config)?;
        vec![(path, kind)]
    };

    let (primary_path, primary_kind) = &version_files[0];
    let primary_doc = read_manifest(primary_path, *primary_kind)?;
    let current = current_version(&primary_doc, *primary_kind)?;

    let target = if let Some(custom_ver) = custom {
        Version::parse(custom_ver)
            .with_context(|| format!("parsing custom version `{}`", custom_ver))?
    } else {
        increment_version(&current, level)?
    };

    let mut staged_paths = Vec::new();

    for (path, kind) in &version_files {
        let mut doc = read_manifest(path, *kind)?;

        if dry_run {
            println!(
                "[dry-run] would update {} from {} to {}",
                path, current, target
            );
        } else {
            write_version(&mut doc, *kind, &target);
            let output = match kind {
                VersionFileKind::PackageJson | VersionFileKind::TauriConf => doc["__raw_json"]
                    .as_str()
                    .map(|s| format!("{}\n", s))
                    .unwrap_or_default(),
                _ => doc.to_string(),
            };
            fs::write(path, output).with_context(|| format!("writing {}", path))?;
            println!("Updated {} to {}", path, target);
        }

        staged_paths.push(path.clone());
    }

    Ok(BumpResult {
        new_version: target,
        changed_paths: staged_paths,
    })
}

fn bump_version(config: &DevConfig, args: &VersionBump, dry_run: bool) -> Result<()> {
    let result = perform_bump(config, args.level, args.custom.as_deref(), dry_run)?;

    let mut staged_paths = result.changed_paths;

    if !args.no_changelog
        && let Some(changelog) = changelog_path(config)?
    {
        update_changelog(&changelog, &result.new_version, dry_run)?;
        staged_paths.push(changelog);
    }

    if !args.no_commit {
        git_add(&staged_paths, dry_run)?;
        let message = format!("chore: release {}", result.new_version);
        git_commit(&message, dry_run)?;
    }

    if args.tag {
        let tag_name = format!("v{}", result.new_version);
        git_tag(&tag_name, dry_run)?;
    }

    Ok(())
}

fn print_changelog(_config: &DevConfig, args: &ChangelogArgs) -> Result<()> {
    let range = if let Some(since) = &args.since {
        format!("{}..HEAD", since)
    } else if args.unreleased {
        let tag = latest_tag().unwrap_or_else(|| "HEAD^".to_string());
        format!("{}..HEAD", tag)
    } else {
        format!("{}..HEAD", DEFAULT_BASE_BRANCH)
    };

    let commits = collect_commit_subjects(&range)?;
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
    let contents = fs::read_to_string(path).with_context(|| format!("reading {}", path))?;
    match kind {
        VersionFileKind::CargoToml | VersionFileKind::PyprojectToml => contents
            .parse::<DocumentMut>()
            .with_context(|| format!("parsing {}", path)),
        VersionFileKind::PackageJson | VersionFileKind::TauriConf => {
            // Store raw JSON in a pseudo-TOML doc for uniform handling
            let mut doc = DocumentMut::new();
            doc["__raw_json"] = toml_edit::value(contents);
            Ok(doc)
        }
    }
}

fn current_version(doc: &DocumentMut, kind: VersionFileKind) -> Result<Version> {
    match kind {
        VersionFileKind::CargoToml => {
            let ver_str = doc
                .get("package")
                .and_then(|p| p.get("version"))
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    anyhow!(
                        "missing package.version in Cargo.toml \
                         (workspace roots need git.version_file in config)"
                    )
                })?;
            Version::parse(ver_str).with_context(|| format!("parsing version `{}`", ver_str))
        }
        VersionFileKind::PyprojectToml => {
            let ver_str = doc
                .get("project")
                .and_then(|p| p.get("version"))
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow!("missing project.version in pyproject.toml"))?;
            Version::parse(ver_str).with_context(|| format!("parsing version `{}`", ver_str))
        }
        VersionFileKind::PackageJson => {
            let raw = doc["__raw_json"]
                .as_str()
                .ok_or_else(|| anyhow!("internal error: missing raw JSON"))?;
            let json: serde_json::Value =
                serde_json::from_str(raw).context("parsing package.json")?;
            let ver_str = json["version"]
                .as_str()
                .ok_or_else(|| anyhow!("missing version in package.json"))?;
            Version::parse(ver_str).with_context(|| format!("parsing version `{}`", ver_str))
        }
        VersionFileKind::TauriConf => {
            let raw = doc["__raw_json"]
                .as_str()
                .ok_or_else(|| anyhow!("internal error: missing raw JSON"))?;
            let json: serde_json::Value =
                serde_json::from_str(raw).context("parsing tauri.conf.json")?;
            let ver_str = json["version"]
                .as_str()
                .ok_or_else(|| anyhow!("missing version in tauri.conf.json"))?;
            Version::parse(ver_str).with_context(|| format!("parsing version `{}`", ver_str))
        }
    }
}

fn write_version(doc: &mut DocumentMut, kind: VersionFileKind, version: &Version) {
    match kind {
        VersionFileKind::CargoToml => doc["package"]["version"] = value(version.to_string()),
        VersionFileKind::PyprojectToml => doc["project"]["version"] = value(version.to_string()),
        VersionFileKind::PackageJson | VersionFileKind::TauriConf => {
            // Update version in the stored raw JSON
            if let Some(raw) = doc["__raw_json"].as_str()
                && let Ok(mut json) = serde_json::from_str::<serde_json::Value>(raw)
            {
                json["version"] = serde_json::Value::String(version.to_string());
                if let Ok(updated) = serde_json::to_string_pretty(&json) {
                    doc["__raw_json"] = value(updated);
                }
            }
        }
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

    // Explicit version_file in config takes precedence
    if let Some(raw) = config
        .git
        .as_ref()
        .and_then(|git| git.version_file.as_deref())
    {
        let path = resolve_path(&cwd, raw)?;
        let kind = detect_version_file(&path)?;
        return Ok((path, kind));
    }

    // Check if this is a Tauri project (has src-tauri/tauri.conf.json)
    let tauri_conf = cwd.join("src-tauri").join("tauri.conf.json");
    if tauri_conf.exists() {
        return Ok((tauri_conf, VersionFileKind::TauriConf));
    }

    // Fall back to default based on configured language
    let (filename, kind) = match config.default_language.as_deref() {
        Some("python") => ("pyproject.toml", VersionFileKind::PyprojectToml),
        Some("typescript" | "javascript") => ("package.json", VersionFileKind::PackageJson),
        _ => ("Cargo.toml", VersionFileKind::CargoToml),
    };
    let path = cwd.join(filename);

    // For Cargo.toml, check if this is a workspace root without a direct package version
    if matches!(kind, VersionFileKind::CargoToml)
        && path.exists()
        && let Some(member_path) = find_workspace_member_version(&cwd, &path)?
    {
        return Ok((member_path, kind));
    }

    Ok((path, kind))
}

/// When a Cargo.toml is a workspace root without its own [package] version,
/// find the first workspace member that has a version.
fn find_workspace_member_version(
    cwd: &Utf8Path,
    cargo_toml: &Utf8Path,
) -> Result<Option<Utf8PathBuf>> {
    let contents =
        fs::read_to_string(cargo_toml).with_context(|| format!("reading {}", cargo_toml))?;
    let doc: DocumentMut = contents
        .parse()
        .with_context(|| format!("parsing {}", cargo_toml))?;

    // Only applies if this is a workspace manifest
    if doc.get("workspace").is_none() {
        return Ok(None);
    }

    // If it also has a [package] with version, it's fine to use as-is
    if doc
        .get("package")
        .and_then(|p| p.get("version"))
        .and_then(|v| v.as_str())
        .is_some()
    {
        return Ok(None);
    }

    // Get workspace members and find the first one with a version
    let members = doc
        .get("workspace")
        .and_then(|w| w.get("members"))
        .and_then(|m| m.as_array())
        .ok_or_else(|| anyhow!("workspace manifest has no members array"))?;

    for member in members.iter() {
        let Some(member_str) = member.as_str() else {
            continue;
        };
        let member_cargo = cwd.join(member_str).join("Cargo.toml");
        if member_cargo.exists() {
            let member_contents = fs::read_to_string(&member_cargo)
                .with_context(|| format!("reading {}", member_cargo))?;
            let member_doc: DocumentMut = member_contents
                .parse()
                .with_context(|| format!("parsing {}", member_cargo))?;
            if member_doc
                .get("package")
                .and_then(|p| p.get("version"))
                .and_then(|v| v.as_str())
                .is_some()
            {
                return Ok(Some(member_cargo));
            }
        }
    }

    Ok(None)
}

/// For Tauri projects, returns all version files that should be updated together
fn locate_tauri_version_files(cwd: &Utf8Path) -> Option<Vec<(Utf8PathBuf, VersionFileKind)>> {
    let tauri_conf = cwd.join("src-tauri").join("tauri.conf.json");
    if !tauri_conf.exists() {
        return None;
    }

    let mut files = vec![(tauri_conf, VersionFileKind::TauriConf)];

    // Also update package.json if it exists
    let package_json = cwd.join("package.json");
    if package_json.exists() {
        files.push((package_json, VersionFileKind::PackageJson));
    }

    // Also update src-tauri/Cargo.toml if it exists
    let cargo_toml = cwd.join("src-tauri").join("Cargo.toml");
    if cargo_toml.exists() {
        files.push((cargo_toml, VersionFileKind::CargoToml));
    }

    Some(files)
}

fn detect_version_file(path: &Utf8Path) -> Result<VersionFileKind> {
    match path.file_name() {
        Some("Cargo.toml") => Ok(VersionFileKind::CargoToml),
        Some("pyproject.toml") => Ok(VersionFileKind::PyprojectToml),
        Some("package.json") => Ok(VersionFileKind::PackageJson),
        Some("tauri.conf.json") => Ok(VersionFileKind::TauriConf),
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

    prepend_release_section(path, &section)
}

pub fn changelog_path(config: &DevConfig) -> Result<Option<Utf8PathBuf>> {
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

#[derive(Clone, Copy)]
enum VersionFileKind {
    CargoToml,
    PyprojectToml,
    PackageJson,
    TauriConf,
}

const DEFAULT_BASE_BRANCH: &str = "release-candidate";
