use std::process::Command;

use anyhow::{Context, Result, anyhow, bail};

pub(crate) fn run_git(args: &[String], dry_run: bool) -> Result<()> {
    if dry_run {
        println!("[dry-run] git {}", args.join(" "));
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

pub(crate) fn run_steps(steps: &[Vec<String>], dry_run: bool) -> Result<()> {
    for step in steps {
        if step.is_empty() {
            continue;
        }

        let display = step.join(" ");
        if dry_run {
            println!("[dry-run] {}", display);
            continue;
        }

        let status = Command::new(&step[0])
            .args(&step[1..])
            .status()
            .with_context(|| format!("running `{}`", display))?;
        if !status.success() {
            let code = status.code().unwrap_or(-1);
            bail!("command `{}` failed with status {}", display, code);
        }
    }
    Ok(())
}

pub(crate) fn ensure_clean_worktree() -> Result<()> {
    let output = Command::new("git")
        .args(["status", "--porcelain"])
        .output()
        .context("checking git status")?;
    if !output.status.success() {
        bail!("git status --porcelain exited with {}", output.status);
    }
    if !output.stdout.is_empty() {
        let status = String::from_utf8_lossy(&output.stdout);
        let preview = status
            .lines()
            .take(8)
            .map(str::to_owned)
            .collect::<Vec<_>>()
            .join("\n");
        return Err(anyhow!(
            "working tree has uncommitted changes; commit, stash, or pass --allow-dirty to override\n{}",
            preview
        ));
    }
    Ok(())
}

pub(crate) fn local_branch_exists(branch: &str) -> Result<bool> {
    let status = Command::new("git")
        .args(["show-ref", "--verify", "--quiet"])
        .arg(format!("refs/heads/{branch}"))
        .status()
        .with_context(|| format!("checking local branch {branch}"))?;
    Ok(status.success())
}

pub(crate) fn remote_exists(remote: &str) -> Result<bool> {
    let output = Command::new("git")
        .args(["remote"])
        .output()
        .context("listing git remotes")?;
    if !output.status.success() {
        let code = output.status.code().unwrap_or(-1);
        bail!("git remote failed with status {}", code);
    }

    let remotes = String::from_utf8_lossy(&output.stdout);
    Ok(remotes.lines().any(|line| line.trim() == remote))
}

pub(crate) fn remote_branch_exists(remote: &str, branch: &str) -> Result<bool> {
    let status = Command::new("git")
        .args(["show-ref", "--verify", "--quiet"])
        .arg(format!("refs/remotes/{remote}/{branch}"))
        .status()
        .with_context(|| format!("checking remote branch {remote}/{branch}"))?;
    Ok(status.success())
}

pub(crate) fn remote_default_branch(remote: &str) -> Result<Option<String>> {
    let output = Command::new("git")
        .args([
            "symbolic-ref",
            "--quiet",
            "--short",
            &format!("refs/remotes/{remote}/HEAD"),
        ])
        .output()
        .with_context(|| format!("checking default branch for remote {remote}"))?;

    if !output.status.success() {
        return Ok(None);
    }

    let reference = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    let prefix = format!("{remote}/");
    Ok(reference.strip_prefix(&prefix).map(str::to_owned))
}

pub(crate) fn current_branch() -> Result<Option<String>> {
    let output = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .context("determining current branch")?;
    if !output.status.success() {
        let code = output.status.code().unwrap_or(-1);
        bail!("git rev-parse failed with status {}", code);
    }
    let branch = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if branch == "HEAD" {
        return Ok(None);
    }
    Ok(Some(branch))
}

pub(crate) fn collect_commit_subjects(range: &str) -> Result<Vec<String>> {
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

pub(crate) fn latest_tag() -> Option<String> {
    let output = Command::new("git")
        .args(["describe", "--tags", "--abbrev=0"])
        .output();
    match output {
        Ok(out) if out.status.success() => {
            Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
        }
        _ => None,
    }
}
