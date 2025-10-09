use anyhow::{Context, Result, anyhow, bail};
use std::process::Command;

const DEFAULT_BASE_BRANCH: &str = "release-candidate";
const DEFAULT_MAIN_BRANCH: &str = "main";

use crate::cli::{BranchCreate, BranchFinalize, ReleasePr};
use crate::config::DevConfig;

pub fn branch_create(args: &BranchCreate, dry_run: bool) -> Result<()> {
    if !args.allow_dirty && !dry_run {
        ensure_clean_worktree()?;
    }

    let _ = args.push; // retained for CLI compatibility; upstream push now always occurs.

    let base = args.base.as_deref().unwrap_or(DEFAULT_BASE_BRANCH);
    let mut steps: Vec<Vec<String>> = vec![
        vec![
            "git".into(),
            "fetch".into(),
            "--all".into(),
            "--prune".into(),
        ],
        vec!["git".into(), "checkout".into(), base.into()],
        vec![
            "git".into(),
            "pull".into(),
            "--rebase".into(),
            "origin".into(),
            base.into(),
        ],
        vec![
            "git".into(),
            "checkout".into(),
            "-B".into(),
            args.name.clone(),
            base.into(),
        ],
    ];

    steps.push(vec![
        "git".into(),
        "push".into(),
        "--set-upstream".into(),
        "origin".into(),
        args.name.clone(),
    ]);

    run_steps(&steps, dry_run)?;
    println!(
        "Branch `{}` created from `{}` and pushed to origin.",
        args.name, base
    );
    Ok(())
}

pub fn branch_finalize(args: &BranchFinalize, dry_run: bool) -> Result<()> {
    if !args.allow_dirty && !dry_run {
        ensure_clean_worktree()?;
    }

    let branch = match &args.name {
        Some(name) => name.clone(),
        None => current_branch()?.ok_or_else(|| anyhow!("unable to determine current branch"))?,
    };
    let base = args.base.as_deref().unwrap_or(DEFAULT_BASE_BRANCH);
    let mut steps: Vec<Vec<String>> = vec![
        vec![
            "git".into(),
            "fetch".into(),
            "--all".into(),
            "--prune".into(),
        ],
        vec!["git".into(), "checkout".into(), base.into()],
        vec![
            "git".into(),
            "pull".into(),
            "--rebase".into(),
            "origin".into(),
            base.into(),
        ],
        vec![
            "git".into(),
            "merge".into(),
            "--no-ff".into(),
            branch.clone(),
        ],
        vec!["git".into(), "push".into(), "origin".into(), base.into()],
    ];

    if args.delete {
        steps.push(vec![
            "git".into(),
            "branch".into(),
            "-d".into(),
            branch.clone(),
        ]);
        steps.push(vec![
            "git".into(),
            "push".into(),
            "origin".into(),
            "--delete".into(),
            branch.clone(),
        ]);
    }

    run_steps(&steps, dry_run)?;
    let deleted = if args.delete { " and deleted" } else { "" };
    println!("Merged `{}` into `{}`{}.", branch, base, deleted);
    Ok(())
}

pub fn release_pr(args: &ReleasePr, dry_run: bool, config: &DevConfig) -> Result<()> {
    let base = args
        .from
        .as_deref()
        .or_else(|| {
            config
                .git
                .as_ref()
                .and_then(|git| git.main_branch.as_deref())
        })
        .unwrap_or(DEFAULT_MAIN_BRANCH);
    let head = args
        .to
        .as_deref()
        .or_else(|| {
            config
                .git
                .as_ref()
                .and_then(|git| git.release_branch.as_deref())
        })
        .unwrap_or(DEFAULT_BASE_BRANCH);

    let commits = collect_commits(base, head)?;
    if commits.is_empty() {
        println!(
            "No commits between {} and {}; skipping PR creation.",
            base, head
        );
        return Ok(());
    }
    update_changelog(base, head, &commits, dry_run)?;

    let mut steps = vec![vec![
        "git".into(),
        "fetch".into(),
        "--all".into(),
        "--prune".into(),
    ]];
    steps.push(vec!["git".into(), "checkout".into(), head.into()]);
    steps.push(vec![
        "git".into(),
        "pull".into(),
        "--rebase".into(),
        "origin".into(),
        head.into(),
    ]);
    steps.push(vec![
        "git".into(),
        "push".into(),
        "origin".into(),
        head.into(),
    ]);
    steps.push(vec![
        "gh".into(),
        "pr".into(),
        "create".into(),
        "--base".into(),
        base.into(),
        "--head".into(),
        head.into(),
        "--fill".into(),
    ]);
    if args.no_open {
        if let Some(step) = steps.last_mut() {
            step.push("--no-open".into());
        }
    }

    run_steps(&steps, dry_run)?;
    println!("Prepared release PR from `{}` into `{}`.", head, base);
    Ok(())
}

fn run_steps(steps: &[Vec<String>], dry_run: bool) -> Result<()> {
    for step in steps {
        let display = step.join(" ");
        if dry_run {
            println!("[dry-run] {}", display);
            continue;
        }
        if step.is_empty() {
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

fn ensure_clean_worktree() -> Result<()> {
    let output = Command::new("git")
        .args(["status", "--porcelain"])
        .output()
        .context("checking git status")?;
    if !output.status.success() {
        bail!("git status --porcelain exited with {}", output.status);
    }
    if !output.stdout.is_empty() {
        return Err(anyhow!(
            "working tree has uncommitted changes; pass --allow-dirty to override"
        ));
    }
    Ok(())
}

fn current_branch() -> Result<Option<String>> {
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

fn collect_commits(base: &str, head: &str) -> Result<Vec<String>> {
    let range = format!("{}..{}", base, head);
    let output = Command::new("git")
        .args(["log", &range, "--pretty=format:%s"])
        .output()
        .with_context(|| format!("collecting commits for {}", range))?;
    if !output.status.success() {
        let code = output.status.code().unwrap_or(-1);
        bail!("git log failed with status {}", code);
    }
    let commits = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    Ok(commits)
}

fn update_changelog(base: &str, head: &str, commits: &[String], dry_run: bool) -> Result<()> {
    if commits.is_empty() {
        println!("No commits between {} and {}.", base, head);
        return Ok(());
    }

    use chrono::Utc;
    use std::fs;
    use std::io::Write;

    let changelog_path = std::env::current_dir()
        .context("reading current directory")?
        .join("CHANGELOG.md");

    let mut section = String::new();
    let date = Utc::now().format("%Y-%m-%d");
    section.push_str(&format!("## {} ({base} → {head})\n\n", date));
    for commit in commits {
        section.push_str("- ");
        section.push_str(commit);
        section.push('\n');
    }
    section.push('\n');

    if dry_run {
        println!(
            "[dry-run] update {} with:\n{}",
            changelog_path.display(),
            section
        );
        return Ok(());
    }

    let mut existing = if changelog_path.exists() {
        fs::read_to_string(&changelog_path)
            .with_context(|| format!("reading {}", changelog_path.display()))?
    } else {
        String::from("# Changelog\n\n")
    };

    existing.push_str(&section);
    fs::File::create(&changelog_path)
        .with_context(|| format!("opening {}", changelog_path.display()))?
        .write_all(existing.as_bytes())
        .with_context(|| format!("writing {}", changelog_path.display()))?;
    Ok(())
}
