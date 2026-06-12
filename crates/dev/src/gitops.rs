use anyhow::{Result, anyhow};
use camino::Utf8Path;

const DEFAULT_BASE_BRANCH: &str = "release-candidate";
const DEFAULT_MAIN_BRANCH: &str = "main";
const DEFAULT_REMOTE: &str = "origin";

use crate::cli::{BranchCreate, BranchFinalize, ReleasePr};
use crate::config::DevConfig;
use crate::core::changelog::prepend_release_section;
use crate::core::git::{
    collect_commit_subjects, current_branch, ensure_clean_worktree, local_branch_exists,
    remote_branch_exists, remote_default_branch, remote_exists, run_steps,
};
use crate::versioning;

pub fn branch_create(args: &BranchCreate, dry_run: bool, config: &DevConfig) -> Result<()> {
    if !args.allow_dirty && !dry_run {
        ensure_clean_worktree()?;
    }

    let base = resolve_workflow_base(args.base.as_deref(), config)?;
    let has_origin = remote_exists(DEFAULT_REMOTE)?;
    let mut steps: Vec<Vec<String>> = Vec::new();

    if has_origin {
        steps.push(vec![
            "git".into(),
            "fetch".into(),
            "--all".into(),
            "--prune".into(),
        ]);
    }

    steps.push(vec!["git".into(), "checkout".into(), base.clone()]);

    if has_origin && remote_branch_exists(DEFAULT_REMOTE, &base)? {
        steps.push(vec![
            "git".into(),
            "pull".into(),
            "--rebase".into(),
            "--autostash".into(),
            DEFAULT_REMOTE.into(),
            base.clone(),
        ]);
    }

    steps.push(vec![
        "git".into(),
        "checkout".into(),
        "-B".into(),
        args.name.clone(),
        base.clone(),
    ]);

    if args.push {
        steps.push(vec![
            "git".into(),
            "push".into(),
            "--set-upstream".into(),
            DEFAULT_REMOTE.into(),
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

pub fn branch_finalize(args: &BranchFinalize, dry_run: bool, config: &DevConfig) -> Result<()> {
    if !args.allow_dirty && !dry_run {
        ensure_clean_worktree()?;
    }

    let branch = match &args.name {
        Some(name) => name.clone(),
        None => current_branch()?.ok_or_else(|| anyhow!("unable to determine current branch"))?,
    };
    let base = resolve_workflow_base(args.base.as_deref(), config)?;
    if branch == base {
        return Err(anyhow!(
            "cannot finalize `{}` into itself; checkout a feature branch or pass --into <base>",
            branch
        ));
    }

    // Push the branch first to ensure it's up to date on remote
    let mut steps: Vec<Vec<String>> = Vec::new();
    if remote_exists(DEFAULT_REMOTE)? {
        steps.push(vec![
            "git".into(),
            "fetch".into(),
            "--all".into(),
            "--prune".into(),
        ]);
    }
    steps.push(vec![
        "git".into(),
        "push".into(),
        "-u".into(),
        DEFAULT_REMOTE.into(),
        branch.clone(),
    ]);
    steps.push(vec![
        "gh".into(),
        "pr".into(),
        "create".into(),
        "--base".into(),
        base.clone(),
        "--head".into(),
        branch.clone(),
        "--fill".into(),
    ]);

    // Warn if --delete was passed (deprecated, deletion now happens via GitHub)
    if args.delete {
        println!(
            "Note: --delete is deprecated. Branch deletion now happens via GitHub after PR merge."
        );
    }

    run_steps(&steps, dry_run)?;
    println!("Created PR for `{}` into `{}`.", branch, base);
    println!("Review and merge via GitHub, then delete the branch if desired.");
    Ok(())
}

fn resolve_workflow_base(explicit_base: Option<&str>, config: &DevConfig) -> Result<String> {
    if let Some(base) = explicit_base {
        return Ok(base.to_owned());
    }

    if let Some(base) = config
        .git
        .as_ref()
        .and_then(|git| git.main_branch.as_deref())
    {
        return Ok(base.to_owned());
    }

    if let Some(base) = remote_default_branch(DEFAULT_REMOTE)? {
        return Ok(base);
    }

    for candidate in ["main", "master"] {
        if local_branch_exists(candidate)? {
            return Ok(candidate.to_owned());
        }
    }

    current_branch()?.ok_or_else(|| {
        anyhow!("unable to determine a base branch; pass --from/--into or set [git].main_branch")
    })
}

pub fn release_pr(args: &ReleasePr, dry_run: bool, config: &DevConfig) -> Result<()> {
    if !dry_run {
        ensure_clean_worktree()?;
    }

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

    // Fetch and ensure we're on the head branch
    run_steps(
        &[
            vec![
                "git".into(),
                "fetch".into(),
                "--all".into(),
                "--prune".into(),
            ],
            vec!["git".into(), "checkout".into(), head.into()],
            vec![
                "git".into(),
                "pull".into(),
                "--rebase".into(),
                "origin".into(),
                head.into(),
            ],
        ],
        dry_run,
    )?;

    let commits = collect_commits(base, head)?;
    if commits.is_empty() {
        println!(
            "No commits between {} and {}; skipping release.",
            base, head
        );
        return Ok(());
    }

    // Bump version
    let bump_result = versioning::perform_bump(config, args.bump, None, dry_run)?;

    // Update changelog with actual commit messages
    let changelog = versioning::changelog_path(config)?;
    if let Some(ref cl) = changelog {
        update_release_changelog(cl, &bump_result.new_version, &commits, dry_run)?;
    }

    // Stage and commit all changes
    let mut paths_to_stage: Vec<String> = bump_result
        .changed_paths
        .iter()
        .map(|p| p.to_string())
        .collect();
    if let Some(ref cl) = changelog {
        paths_to_stage.push(cl.to_string());
    }

    let mut add_args: Vec<String> = vec!["git".into(), "add".into()];
    add_args.extend(paths_to_stage);
    let commit_msg = format!("chore: release v{}", bump_result.new_version);

    run_steps(
        &[
            add_args,
            vec!["git".into(), "commit".into(), "-m".into(), commit_msg],
        ],
        dry_run,
    )?;

    // Push and create PR
    let title = format!("chore: release v{}", bump_result.new_version);
    let mut pr_step = vec![
        "gh".into(),
        "pr".into(),
        "create".into(),
        "--base".into(),
        base.into(),
        "--head".into(),
        head.into(),
        "--title".into(),
        title,
        "--fill".into(),
    ];
    if args.no_open {
        pr_step.push("--no-open".into());
    }

    run_steps(
        &[
            vec!["git".into(), "push".into(), "origin".into(), head.into()],
            pr_step,
        ],
        dry_run,
    )?;

    println!(
        "Released v{} — PR from `{}` into `{}`.",
        bump_result.new_version, head, base
    );
    Ok(())
}

fn collect_commits(base: &str, head: &str) -> Result<Vec<String>> {
    let range = format!("{}..{}", base, head);
    collect_commit_subjects(&range)
}

fn update_release_changelog(
    path: &Utf8Path,
    version: &semver::Version,
    commits: &[String],
    dry_run: bool,
) -> Result<()> {
    use chrono::Utc;
    let date = Utc::now().format("%Y-%m-%d");
    let mut section = format!("## {} - v{}\n\n", date, version);
    for commit in commits {
        section.push_str("- ");
        section.push_str(commit);
        section.push('\n');
    }
    section.push('\n');

    if dry_run {
        println!("[dry-run] update {} with:\n{}", path, section);
        return Ok(());
    }

    prepend_release_section(path, &section)
}
