use std::ffi::OsString;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use semver::Version;
use serde::Deserialize;

use crate::cli::UpdateArgs;
use crate::core::exec::{format_command, run_process};
use crate::dispatch::CliContext;

const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Deserialize)]
struct GitHubRelease {
    tag_name: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Target {
    triple: &'static str,
    archive_ext: &'static str,
}

impl Target {
    fn detect() -> Result<Self> {
        let triple = match (std::env::consts::OS, std::env::consts::ARCH) {
            ("linux", "x86_64") => "x86_64-unknown-linux-gnu",
            ("linux", "aarch64") => "aarch64-unknown-linux-gnu",
            ("macos", "x86_64") => "x86_64-apple-darwin",
            ("macos", "aarch64") => "aarch64-apple-darwin",
            (os, arch) => bail!(
                "self-update is not available for {os}/{arch}; install with cargo or download a release asset manually"
            ),
        };

        Ok(Self {
            triple,
            archive_ext: "tar.gz",
        })
    }
}

pub(crate) fn handle(ctx: &CliContext, args: UpdateArgs) -> Result<()> {
    validate_repo(&args.repo)?;
    let target = Target::detect()?;
    let desired_tag = match args.version.as_deref() {
        Some(version) => normalize_tag(version),
        None => fetch_latest_tag(&args.repo)?,
    };
    let desired_version = parse_tag_version(&desired_tag)?;
    let current_version = Version::parse(CURRENT_VERSION)
        .with_context(|| format!("parsing current version {CURRENT_VERSION}"))?;

    println!("Current dev version: v{current_version}");
    println!("Target dev version:  {desired_tag}");

    if args.check {
        if desired_version > current_version {
            println!("Update available: v{current_version} -> {desired_tag}");
        } else if desired_version == current_version {
            println!("dev is up to date.");
        } else {
            println!("Requested release {desired_tag} is older than current v{current_version}.");
        }
        return Ok(());
    }

    if desired_version <= current_version && args.version.is_none() {
        println!("dev is up to date.");
        return Ok(());
    }

    let install_dir = resolve_install_dir(args.install_dir.as_deref())?;
    let asset_name = format!("dev-{desired_tag}-{}.{}", target.triple, target.archive_ext);
    let url = format!(
        "https://github.com/{}/releases/download/{}/{}",
        args.repo, desired_tag, asset_name
    );

    println!("Release asset: {url}");
    println!("Install dir:   {}", install_dir.display());

    if ctx.dry_run {
        println!(
            "Dry run: would download, extract, and install dev to {}",
            install_dir.display()
        );
        return Ok(());
    }

    if !args.yes && !confirm("Install this release?")? {
        println!("Update canceled.");
        return Ok(());
    }

    install_release(&url, &install_dir, &desired_tag, &asset_name)
}

fn validate_repo(repo: &str) -> Result<()> {
    let mut parts = repo.split('/');
    let owner = parts.next().unwrap_or_default();
    let name = parts.next().unwrap_or_default();
    if owner.is_empty()
        || name.is_empty()
        || parts.next().is_some()
        || !repo
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '/'))
    {
        bail!("repository must be in owner/repo form");
    }
    Ok(())
}

fn normalize_tag(version: &str) -> String {
    if version.starts_with('v') {
        version.to_owned()
    } else {
        format!("v{version}")
    }
}

fn parse_tag_version(tag: &str) -> Result<Version> {
    Version::parse(tag.trim_start_matches('v'))
        .with_context(|| format!("parsing release tag {tag}"))
}

fn fetch_latest_tag(repo: &str) -> Result<String> {
    let url = format!("https://api.github.com/repos/{repo}/releases/latest");
    let output = std::process::Command::new("curl")
        .args(["-fsSL", "-H", "Accept: application/vnd.github+json", &url])
        .output()
        .with_context(|| "running curl to query GitHub releases")?;

    if !output.status.success() {
        bail!(
            "failed to query latest release for {repo}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }

    latest_tag_from_json(&output.stdout)
}

fn latest_tag_from_json(bytes: &[u8]) -> Result<String> {
    let release: GitHubRelease =
        serde_json::from_slice(bytes).context("parsing GitHub release JSON")?;
    if release.tag_name.trim().is_empty() {
        bail!("latest release JSON did not contain tag_name");
    }
    Ok(release.tag_name)
}

fn resolve_install_dir(explicit: Option<&Path>) -> Result<PathBuf> {
    if let Some(path) = explicit {
        return Ok(expand_home(path));
    }

    if let Ok(current_exe) = std::env::current_exe()
        && let Some(parent) = current_exe.parent()
        && is_user_writable_candidate(parent)
    {
        return Ok(parent.to_path_buf());
    }

    let home = dirs::home_dir().ok_or_else(|| anyhow!("unable to determine home directory"))?;
    Ok(home.join(".local").join("bin"))
}

fn expand_home(path: &Path) -> PathBuf {
    let display = path.to_string_lossy();
    if display == "~" {
        return dirs::home_dir().unwrap_or_else(|| PathBuf::from("~"));
    }
    if let Some(rest) = display.strip_prefix("~/") {
        return dirs::home_dir()
            .map(|home| home.join(rest))
            .unwrap_or_else(|| path.to_path_buf());
    }
    path.to_path_buf()
}

fn is_user_writable_candidate(path: &Path) -> bool {
    path.starts_with(dirs::home_dir().unwrap_or_default()) || path.starts_with("/usr/local/bin")
}

fn confirm(prompt: &str) -> Result<bool> {
    print!("{prompt} [y/N] ");
    io::stdout().flush().context("flushing stdout")?;
    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .context("reading confirmation")?;
    Ok(matches!(input.trim(), "y" | "Y" | "yes" | "YES" | "Yes"))
}

fn install_release(
    url: &str,
    install_dir: &Path,
    desired_tag: &str,
    asset_name: &str,
) -> Result<()> {
    fs::create_dir_all(install_dir)
        .with_context(|| format!("creating install dir {}", install_dir.display()))?;

    let temp_root = std::env::temp_dir().join(format!(
        "dev-update-{}-{}",
        std::process::id(),
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ));
    fs::create_dir_all(&temp_root).with_context(|| format!("creating {}", temp_root.display()))?;

    let archive = temp_root.join("dev.tar.gz");
    run_checked(&[
        OsString::from("curl"),
        OsString::from("-fL"),
        OsString::from("--proto"),
        OsString::from("=https"),
        OsString::from("--tlsv1.2"),
        OsString::from("-o"),
        archive.as_os_str().to_owned(),
        OsString::from(url),
    ])?;

    verify_checksum_if_available(url, asset_name, &archive, &temp_root)?;

    run_checked(&[
        OsString::from("tar"),
        OsString::from("-xzf"),
        archive.as_os_str().to_owned(),
        OsString::from("-C"),
        temp_root.as_os_str().to_owned(),
    ])?;

    let extracted = temp_root.join("dev");
    if !extracted.exists() {
        bail!("release archive did not contain a dev binary at its root");
    }

    let target = install_dir.join("dev");
    let backup = install_dir.join("dev.old");
    if target.exists() {
        let _ = fs::remove_file(&backup);
        fs::rename(&target, &backup).with_context(|| {
            format!(
                "moving existing binary from {} to {}",
                target.display(),
                backup.display()
            )
        })?;
    }

    fs::copy(&extracted, &target)
        .with_context(|| format!("installing binary to {}", target.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&target, fs::Permissions::from_mode(0o755))
            .with_context(|| format!("marking {} executable", target.display()))?;
    }

    let verify = std::process::Command::new(&target)
        .arg("--version")
        .output();
    match verify {
        Ok(output) if output.status.success() => {
            let version_output = String::from_utf8_lossy(&output.stdout);
            if !version_output.contains(desired_tag.trim_start_matches('v')) {
                bail!(
                    "installed binary version did not match {desired_tag}: {}",
                    version_output.trim()
                );
            }
        }
        Ok(output) => bail!(
            "installed binary failed verification: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ),
        Err(error) => bail!("failed to run installed binary: {error}"),
    }

    let _ = fs::remove_dir_all(&temp_root);
    println!("Updated dev to {desired_tag} at {}", target.display());
    if backup.exists() {
        println!("Previous binary saved at {}", backup.display());
    }
    Ok(())
}

fn verify_checksum_if_available(
    asset_url: &str,
    asset_name: &str,
    archive: &Path,
    temp_root: &Path,
) -> Result<()> {
    let Some(base_url) = asset_url.rsplit_once('/').map(|(base, _)| base) else {
        return Ok(());
    };
    let checksums_url = format!("{base_url}/checksums.txt");
    let checksums_path = temp_root.join("checksums.txt");

    let curl_status = std::process::Command::new("curl")
        .args(["-fsSL", "-o"])
        .arg(&checksums_path)
        .arg(&checksums_url)
        .status();

    match curl_status {
        Ok(status) if status.success() => {}
        _ => {
            println!("[warn] checksums.txt unavailable; skipping checksum verification");
            return Ok(());
        }
    }

    let checksums = fs::read_to_string(&checksums_path)
        .with_context(|| format!("reading {}", checksums_path.display()))?;
    let Some(expected) = checksums.lines().find_map(|line| {
        let mut parts = line.split_whitespace();
        let hash = parts.next()?;
        let name = parts.next()?;
        (name == asset_name).then_some(hash.to_owned())
    }) else {
        println!(
            "[warn] checksums.txt did not contain {asset_name}; skipping checksum verification"
        );
        return Ok(());
    };

    let output = std::process::Command::new("sha256sum")
        .arg(archive)
        .output();
    let output = match output {
        Ok(output) if output.status.success() => output,
        _ => {
            println!("[warn] sha256sum unavailable; skipping checksum verification");
            return Ok(());
        }
    };
    let actual = String::from_utf8_lossy(&output.stdout)
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_owned();
    if actual != expected {
        bail!("checksum mismatch for {asset_name}");
    }
    println!("Verified checksum for {asset_name}");
    Ok(())
}

fn run_checked(argv: &[OsString]) -> Result<()> {
    let display_args = argv
        .iter()
        .map(|arg| arg.to_string_lossy().to_string())
        .collect::<Vec<_>>();
    println!("  -> {}", format_command(&display_args));
    let status = run_process(&display_args)?;
    if !status.success() {
        bail!("command failed: {}", format_command(&display_args));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_latest_tag_json() {
        let tag = latest_tag_from_json(br#"{"tag_name":"v1.2.3"}"#).expect("tag");
        assert_eq!(tag, "v1.2.3");
    }

    #[test]
    fn validates_repo_names() {
        validate_repo("owner/repo").expect("valid");
        assert!(validate_repo("owner").is_err());
        assert!(validate_repo("owner/repo/extra").is_err());
        assert!(validate_repo("owner/re po").is_err());
    }

    #[test]
    fn normalizes_version_tags() {
        assert_eq!(normalize_tag("1.2.3"), "v1.2.3");
        assert_eq!(normalize_tag("v1.2.3"), "v1.2.3");
    }
}
