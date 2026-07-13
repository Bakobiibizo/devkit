use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, anyhow, bail};
use globset::{Glob, GlobSet, GlobSetBuilder};
use regex::Regex;
use serde::Deserialize;

use crate::cli::GuardFormat;

pub(crate) struct GuardOptions {
    pub(crate) base: String,
    pub(crate) head: String,
    pub(crate) config: PathBuf,
    pub(crate) format: GuardFormat,
    pub(crate) rules_from_worktree: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct GuardConfig {
    #[serde(default = "config_version")]
    version: u32,
    rules: Vec<RuleConfig>,
}

const fn config_version() -> u32 {
    1
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
enum Severity {
    Deny,
    Warn,
}

impl Severity {
    fn marker(self) -> &'static str {
        match self {
            Self::Deny => "error",
            Self::Warn => "warn",
        }
    }

    fn github_level(self) -> &'static str {
        match self {
            Self::Deny => "error",
            Self::Warn => "warning",
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RuleConfig {
    id: String,
    severity: Severity,
    pattern: String,
    message: String,
    #[serde(default)]
    guidance: Option<String>,
    #[serde(default)]
    include: Vec<String>,
    #[serde(default)]
    exclude: Vec<String>,
}

struct Rule {
    id: String,
    severity: Severity,
    regex: Regex,
    message: String,
    guidance: Option<String>,
    include: GlobSet,
    exclude: GlobSet,
}

#[derive(Debug, Eq, PartialEq)]
struct AddedLine {
    path: String,
    number: usize,
    content: String,
}

struct Finding<'a> {
    rule: &'a Rule,
    line: &'a AddedLine,
}

fn git(repo_root: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .arg("-c")
        .arg("core.quotePath=false")
        .args(args)
        .current_dir(repo_root)
        .output()
        .with_context(|| format!("running git {}", args.join(" ")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("git {} failed: {}", args.join(" "), stderr.trim());
    }

    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn repo_root() -> Result<PathBuf> {
    let output = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .context("running git rev-parse --show-toplevel")?;
    if !output.status.success() {
        bail!("dev guard must run inside a git repository");
    }
    Ok(PathBuf::from(
        String::from_utf8_lossy(&output.stdout).trim(),
    ))
}

fn parse_new_start(header: &str) -> Option<usize> {
    let range = header
        .split_whitespace()
        .find(|part| part.starts_with('+'))?;
    range
        .trim_start_matches('+')
        .split(',')
        .next()?
        .parse()
        .ok()
}

fn parse_added_lines(diff: &str) -> Vec<AddedLine> {
    let mut lines = Vec::new();
    let mut path: Option<String> = None;
    let mut next_line: Option<usize> = None;

    for raw in diff.lines() {
        if let Some(value) = raw.strip_prefix("+++ b/") {
            path = Some(value.to_owned());
            next_line = None;
            continue;
        }
        if raw == "+++ /dev/null" {
            path = None;
            next_line = None;
            continue;
        }
        if raw.starts_with("@@") {
            next_line = parse_new_start(raw);
            continue;
        }

        let (Some(current_path), Some(number)) = (path.as_ref(), next_line) else {
            continue;
        };

        if let Some(content) = raw.strip_prefix('+') {
            lines.push(AddedLine {
                path: current_path.clone(),
                number,
                content: content.to_owned(),
            });
            next_line = Some(number + 1);
        } else if raw.starts_with('-') || raw.starts_with("\\ No newline") {
            // Removed lines and patch metadata do not advance the new-file cursor.
        } else {
            next_line = Some(number + 1);
        }
    }

    lines
}

fn build_globs(patterns: &[String], default_all: bool, rule_id: &str) -> Result<GlobSet> {
    let mut builder = GlobSetBuilder::new();
    if patterns.is_empty() && default_all {
        builder.add(Glob::new("**").expect("static glob is valid"));
    } else {
        for pattern in patterns {
            builder.add(
                Glob::new(pattern)
                    .with_context(|| format!("invalid path glob `{pattern}` in rule {rule_id}"))?,
            );
        }
    }
    builder
        .build()
        .with_context(|| format!("building path globs for rule {rule_id}"))
}

fn compile_rules(config: GuardConfig) -> Result<Vec<Rule>> {
    if config.version != 1 {
        bail!(
            "unsupported guard config version {}; expected 1",
            config.version
        );
    }
    if config.rules.is_empty() {
        bail!("guard config must define at least one [[rules]] entry");
    }

    let mut ids = HashSet::new();
    let mut rules = Vec::with_capacity(config.rules.len());
    for rule in config.rules {
        if rule.id.trim().is_empty() {
            bail!("guard rule IDs cannot be empty");
        }
        if !ids.insert(rule.id.clone()) {
            bail!("duplicate guard rule ID `{}`", rule.id);
        }
        if rule.message.trim().is_empty() {
            bail!("guard rule {} must have a message", rule.id);
        }

        rules.push(Rule {
            regex: Regex::new(&rule.pattern)
                .with_context(|| format!("invalid regex in rule {}", rule.id))?,
            include: build_globs(&rule.include, true, &rule.id)?,
            exclude: build_globs(&rule.exclude, false, &rule.id)?,
            id: rule.id,
            severity: rule.severity,
            message: rule.message,
            guidance: rule.guidance,
        });
    }
    Ok(rules)
}

fn config_repo_path(config_path: &Path, repo_root: &Path) -> Result<String> {
    let absolute = if config_path.is_absolute() {
        config_path.to_path_buf()
    } else {
        repo_root.join(config_path)
    };
    let relative = absolute.strip_prefix(repo_root).map_err(|_| {
        anyhow!(
            "base-revision policy requires --config to be inside the repository; use --rules-from-worktree for an external config"
        )
    })?;
    relative
        .to_str()
        .map(|value| value.replace('\\', "/"))
        .ok_or_else(|| anyhow!("guard config path must be valid UTF-8"))
}

fn read_policy(
    repo_root: &Path,
    config_path: &Path,
    merge_base: &str,
    rules_from_worktree: bool,
) -> Result<(String, &'static str)> {
    if !rules_from_worktree {
        let relative = config_repo_path(config_path, repo_root)?;
        let spec = format!("{merge_base}:{relative}");
        if let Ok(contents) = git(repo_root, &["show", &spec]) {
            return Ok((contents, "base"));
        }
    }

    let absolute = if config_path.is_absolute() {
        config_path.to_path_buf()
    } else {
        repo_root.join(config_path)
    };
    let contents = std::fs::read_to_string(&absolute)
        .with_context(|| format!("reading guard config {}", absolute.display()))?;
    Ok((contents, "worktree"))
}

fn escape_github(value: &str) -> String {
    value
        .replace('%', "%25")
        .replace('\r', "%0D")
        .replace('\n', "%0A")
}

fn escape_github_property(value: &str) -> String {
    escape_github(value).replace(':', "%3A").replace(',', "%2C")
}

fn compact(value: &str, max_chars: usize) -> String {
    let normalized = value.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut chars = normalized.chars();
    let prefix = chars.by_ref().take(max_chars).collect::<String>();
    if chars.next().is_some() {
        format!("{prefix}…")
    } else {
        prefix
    }
}

fn render_finding(finding: &Finding<'_>, format: GuardFormat) -> String {
    let rule = finding.rule;
    let line = finding.line;
    match format {
        GuardFormat::Summary => format!(
            "[{}] {} {}:{} {}",
            rule.severity.marker(),
            rule.id,
            line.path,
            line.number,
            compact(&rule.message, 180)
        ),
        GuardFormat::Github => {
            let detail = match rule.guidance.as_deref() {
                Some(guidance) => format!("{} {}", rule.message, guidance),
                None => rule.message.clone(),
            };
            format!(
                "::{} file={},line={},title=dev guard {}::{}",
                rule.severity.github_level(),
                escape_github_property(&line.path),
                line.number,
                escape_github_property(&rule.id),
                escape_github(&compact(&detail, 300))
            )
        }
        GuardFormat::Detailed => {
            let mut rendered = format!(
                "[{}] {} {}:{}\n  {}\n  > {}",
                rule.severity.marker(),
                rule.id,
                line.path,
                line.number,
                rule.message,
                line.content.trim()
            );
            if let Some(guidance) = &rule.guidance {
                rendered.push_str("\n  Guidance: ");
                rendered.push_str(guidance);
            }
            rendered
        }
    }
}

pub(crate) fn run_guard(options: GuardOptions) -> Result<()> {
    let root = repo_root()?;
    git(&root, &["rev-parse", "--verify", &options.base])?;
    git(&root, &["rev-parse", "--verify", &options.head])?;
    let merge_base = git(&root, &["merge-base", &options.base, &options.head])?
        .trim()
        .to_owned();

    let (policy_toml, policy_source) = read_policy(
        &root,
        &options.config,
        &merge_base,
        options.rules_from_worktree,
    )?;
    let config: GuardConfig = toml::from_str(&policy_toml).context("parsing guard config")?;
    let rules = compile_rules(config)?;

    let diff = git(
        &root,
        &[
            "diff",
            "--unified=0",
            "--no-ext-diff",
            "--no-renames",
            "--diff-filter=ACMR",
            &merge_base,
            &options.head,
            "--",
        ],
    )?;
    let policy_path = config_repo_path(&options.config, &root).ok();
    let policy_changed = policy_path.as_ref().is_some_and(|path| {
        let old_policy_header = format!("--- a/{path}");
        let new_policy_header = format!("+++ b/{path}");
        diff.lines()
            .any(|line| line == old_policy_header || line == new_policy_header)
    });
    let added_lines = parse_added_lines(&diff);

    let mut findings = Vec::new();
    for line in &added_lines {
        for rule in &rules {
            if rule.include.is_match(&line.path)
                && !rule.exclude.is_match(&line.path)
                && rule.regex.is_match(&line.content)
            {
                findings.push(Finding { rule, line });
            }
        }
    }

    findings.sort_by(|left, right| {
        let left_priority = usize::from(left.rule.severity == Severity::Warn);
        let right_priority = usize::from(right.rule.severity == Severity::Warn);
        left_priority
            .cmp(&right_priority)
            .then(left.line.path.cmp(&right.line.path))
            .then(left.line.number.cmp(&right.line.number))
            .then(left.rule.id.cmp(&right.rule.id))
    });

    let denied = findings
        .iter()
        .filter(|finding| finding.rule.severity == Severity::Deny)
        .count();
    let warned = findings.len() - denied;
    let range = format!("{}...{}", options.base, options.head);

    if findings.is_empty() {
        if policy_changed {
            println!(
                "[warn] guard: {} added lines checked against {} rules; no code matches, but {} changed ({range}, {policy_source} policy).",
                added_lines.len(),
                rules.len(),
                policy_path.as_deref().unwrap_or("external guard policy")
            );
            println!("[warn] guard: review the policy change; it cannot affect this check.");
        } else {
            println!(
                "[ok] guard: {} added lines checked against {} rules; no new matches ({range}, {policy_source} policy).",
                added_lines.len(),
                rules.len()
            );
        }
        return Ok(());
    }

    let marker = if denied > 0 { "error" } else { "warn" };
    println!(
        "[{marker}] guard: {denied} blocking, {warned} warning matches in {} added lines ({range}, {policy_source} policy).",
        added_lines.len()
    );
    if policy_changed {
        println!(
            "[warn] guard: {} changed; review it separately because the base policy remains active.",
            policy_path.as_deref().unwrap_or("external guard policy")
        );
    }
    for finding in &findings {
        println!("{}", render_finding(finding, options.format));
    }
    if options.format != GuardFormat::Detailed {
        println!("[hint] rerun with --format detailed for matched source and guidance.");
    }

    if denied > 0 {
        bail!("guard rejected newly added failure-mode matches");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{AddedLine, parse_added_lines};

    #[test]
    fn parses_only_added_lines_with_new_file_numbers() {
        let diff = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,2 +1,3 @@\n old\n+new\n last\n@@ -8 +9,2 @@\n-old again\n+replacement\n+tail\n";
        assert_eq!(
            parse_added_lines(diff),
            vec![
                AddedLine {
                    path: "src/lib.rs".into(),
                    number: 2,
                    content: "new".into(),
                },
                AddedLine {
                    path: "src/lib.rs".into(),
                    number: 9,
                    content: "replacement".into(),
                },
                AddedLine {
                    path: "src/lib.rs".into(),
                    number: 10,
                    content: "tail".into(),
                },
            ]
        );
    }
}
