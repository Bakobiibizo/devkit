//! `dev ci install`: writes a GitHub Actions workflow that runs the self-hosted runner's
//! global `dev ci`, then reviews same-repository pull requests with `agentctl task pr-review`.

use std::process::Command;

use anyhow::{Context, Result, bail};
use camino::Utf8PathBuf;

use crate::cli::{CiInstallArgs, Verb};
use crate::commands::task::pipeline_for_language;
use crate::dispatch::AppState;
use crate::templates;

const WORKFLOW: &str = ".github/workflows/dev-ci.yml";

pub(crate) fn handle_install(state: &AppState, args: CiInstallArgs) -> Result<()> {
    let workflow = render(&args)?;
    let destination = repository_root()?.join(WORKFLOW);

    match state.project_language.as_deref() {
        Some(language) if pipeline_for_language(&state.config, language, Verb::Ci).is_some() => {}
        Some(language) => eprintln!(
            "warning: no `ci` pipeline is configured for `{language}`; the workflow's `dev ci` step will fail until one is"
        ),
        None => eprintln!(
            "warning: no project language resolved from {}; check that `dev ci` runs here before relying on the workflow",
            state.config_path
        ),
    }

    if destination.exists() && !args.force {
        bail!("{destination} already exists; pass --force to overwrite it");
    }
    if state.ctx.dry_run {
        println!("[dry-run] would write {destination}:\n{workflow}");
        return Ok(());
    }

    let existed = destination.exists();
    templates::write_to(&destination, workflow.as_bytes())?;
    println!(
        "{} {destination}",
        if existed { "overwrote" } else { "created" }
    );
    println!(
        "Make the `dev ci` job a required status check before relying on auto-merge from the review job."
    );
    Ok(())
}

/// The workflow text for these arguments.
pub(crate) fn render(args: &CiInstallArgs) -> Result<String> {
    let runs_on = labels(&args.runs_on)?;
    if args.branches.is_empty() || args.branches.iter().any(|branch| !valid_word(branch)) {
        bail!(
            "--branch values must be non-empty and contain no spaces, commas, quotes or brackets"
        );
    }

    let mut workflow = template("ci/dev-ci.yml")?;
    if !args.no_review {
        let mut review_args = String::new();
        if let Some(agent) = &args.review_agent {
            if !valid_word(agent) {
                bail!("--review-agent must be a single profile name");
            }
            review_args.push_str(&format!(" --agent {agent}"));
        }
        if let Some(threshold) = args.confidence_threshold {
            review_args.push_str(&format!(" --confidence-threshold {threshold}"));
        }
        workflow.push_str(&template("ci/pr-review.yml")?.replace("{{REVIEW_ARGS}}", &review_args));
    }

    Ok(workflow
        .replace("{{RUNS_ON}}", &runs_on.join(", "))
        .replace("{{BRANCHES}}", &args.branches.join(", ")))
}

fn template(path: &str) -> Result<String> {
    String::from_utf8(templates::get_bytes(path)?)
        .with_context(|| format!("template {path} is not UTF-8"))
}

fn labels(value: &str) -> Result<Vec<&str>> {
    let labels: Vec<&str> = value
        .split(',')
        .map(str::trim)
        .filter(|label| !label.is_empty())
        .collect();
    if labels.is_empty() || labels.iter().any(|label| !valid_word(label)) {
        bail!("--runs-on needs comma-separated runner labels without spaces, quotes or brackets");
    }
    Ok(labels)
}

fn valid_word(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '/'))
}

/// The enclosing git repository's root, or the current directory outside one.
fn repository_root() -> Result<Utf8PathBuf> {
    let output = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output();
    if let Ok(output) = output
        && output.status.success()
    {
        let root = String::from_utf8(output.stdout).context("git printed a non-UTF-8 path")?;
        return Ok(Utf8PathBuf::from(root.trim()));
    }
    let current = std::env::current_dir().context("reading the current directory")?;
    Utf8PathBuf::from_path_buf(current)
        .map_err(|path| anyhow::anyhow!("current directory {} is not UTF-8", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args() -> CiInstallArgs {
        CiInstallArgs {
            force: false,
            runs_on: "self-hosted,linux,arm64".into(),
            branches: vec!["main".into()],
            no_review: false,
            review_agent: None,
            confidence_threshold: None,
        }
    }

    #[test]
    fn default_workflow_runs_global_dev_and_reviews_on_the_self_hosted_runner() {
        let workflow = render(&args()).unwrap();
        for placeholder in ["{{RUNS_ON}}", "{{BRANCHES}}", "{{REVIEW_ARGS}}"] {
            assert!(!workflow.contains(placeholder), "{placeholder} is filled");
        }
        assert!(workflow.contains("        run: dev ci\n"));
        assert!(!workflow.contains(".dev/bin"), "no vendored binary");
        assert!(!workflow.contains("lfs"));
        assert_eq!(
            workflow
                .matches("runs-on: [self-hosted, linux, arm64]")
                .count(),
            2
        );
        assert!(workflow.contains("    branches: [main]\n"));
        assert!(workflow.contains("    needs: [dev-ci]\n"));
        assert!(workflow.contains("ref: ${{ github.event.pull_request.head.sha }}"));
        assert!(workflow.contains("        run: agentctl task pr-review --json\n"));
        assert!(
            workflow.contains("github.event.pull_request.head.repo.full_name == github.repository")
        );
    }

    #[test]
    fn review_options_and_labels_are_rendered() {
        let mut args = args();
        args.runs_on = " self-hosted , linux,x64 ".into();
        args.branches = vec!["main".into(), "release-candidate".into()];
        args.review_agent = Some("codex".into());
        args.confidence_threshold = Some(95);
        let workflow = render(&args).unwrap();
        assert!(workflow.contains("runs-on: [self-hosted, linux, x64]"));
        assert!(workflow.contains("branches: [main, release-candidate]"));
        assert!(workflow.contains(
            "run: agentctl task pr-review --agent codex --confidence-threshold 95 --json"
        ));
    }

    #[test]
    fn no_review_leaves_out_the_review_job() {
        let mut args = args();
        args.no_review = true;
        let workflow = render(&args).unwrap();
        assert!(!workflow.contains("pr-review"));
        assert!(!workflow.contains("agentctl"));
        assert!(workflow.contains("run: dev ci"));
    }

    #[test]
    fn unsafe_values_are_refused() {
        let mut bad = args();
        bad.runs_on = "self-hosted, ]".into();
        assert!(render(&bad).is_err());
        let mut bad = args();
        bad.runs_on = " , ".into();
        assert!(render(&bad).is_err());
        let mut bad = args();
        bad.branches = vec!["main, evil".into()];
        assert!(render(&bad).is_err());
        let mut bad = args();
        bad.review_agent = Some("codex; rm -rf /".into());
        assert!(render(&bad).is_err());
    }
}
