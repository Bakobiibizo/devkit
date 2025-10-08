use anyhow::{Context, Result, anyhow, bail};
use std::process::Command;

const DEFAULT_BASE_BRANCH: &str = "release-candidate";

use crate::cli::{BranchCreate, BranchFinalize, ReleasePr};

pub fn branch_create(args: &BranchCreate, dry_run: bool) -> Result<()> {
    if !args.allow_dirty && !dry_run {
        ensure_clean_worktree()?;
    }

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

    if args.push {
        steps.push(vec![
            "git".into(),
            "push".into(),
            "--set-upstream".into(),
            "origin".into(),
            args.name.clone(),
        ]);
    }

    run_steps(&steps, dry_run)?;
    let pushed = if args.push {
        " and pushed to origin"
    } else {
        ""
    };
    println!("Branch `{}` created from `{}`{}.", args.name, base, pushed);
    Ok(())
}

pub fn branch_finalize(args: &BranchFinalize, dry_run: bool) -> Result<()> {
    let _ = (args, dry_run);
    // TODO: execute git workflow for finalizing a branch.
    Ok(())
}

pub fn release_pr(args: &ReleasePr, dry_run: bool) -> Result<()> {
    let _ = (args, dry_run);
    // TODO: assemble changelog and open PR via gh.
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
