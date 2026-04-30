use std::collections::BTreeSet;
use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use camino::{Utf8Path, Utf8PathBuf};
use dialoguer::{Confirm, MultiSelect, Select, theme::ColorfulTheme};
use sha2::{Digest, Sha256};

use crate::cli::InitArgs;
use crate::runner::CliContext;
use crate::setup::{Component, SetupConfig, SetupContext};
use crate::templates;

#[derive(Clone, Debug, Eq, PartialEq)]
enum Language {
    Rust,
    Python,
    TypeScript,
}

impl Language {
    fn as_str(&self) -> &'static str {
        match self {
            Language::Rust => "rust",
            Language::Python => "python",
            Language::TypeScript => "typescript",
        }
    }

    fn label(&self) -> &'static str {
        match self {
            Language::Rust => "Rust",
            Language::Python => "Python",
            Language::TypeScript => "TypeScript",
        }
    }
}

pub(crate) fn run(ctx: &CliContext, args: InitArgs) -> Result<()> {
    if args.tooling && args.no_tooling {
        bail!("--tooling and --no-tooling cannot be used together");
    }
    if args.ci && args.no_ci {
        bail!("--ci and --no-ci cannot be used together");
    }
    if args.os_configs && args.no_os_configs {
        bail!("--os-configs and --no-os-configs cannot be used together");
    }

    let theme = ColorfulTheme::default();
    let cwd = env_current_dir()?;
    let detected = detect_languages(&cwd);
    let languages = select_languages(&theme, &args, &detected)?;
    if languages.is_empty() {
        bail!("at least one language is required");
    }

    let install_tooling = if args.no_tooling {
        false
    } else if args.tooling || args.yes {
        args.tooling
    } else {
        Confirm::with_theme(&theme)
            .with_prompt("Install/setup tooling for selected languages?")
            .default(false)
            .interact()?
    };

    let write_ci = if args.global || args.no_ci {
        false
    } else if args.ci || args.yes {
        true
    } else {
        Confirm::with_theme(&theme)
            .with_prompt("Create .github/workflows/dev-ci.yml?")
            .default(true)
            .interact()?
    };

    let write_os_configs = if args.global || args.no_os_configs {
        false
    } else if args.os_configs || args.yes {
        args.os_configs
    } else {
        Confirm::with_theme(&theme)
            .with_prompt("Create OS-specific .dev/config.*.toml variants?")
            .default(false)
            .interact()?
    };

    let root = if args.global {
        home_dir()?
    } else {
        cwd.clone()
    };
    let config_path = if args.global {
        Utf8PathBuf::from_path_buf(home_dir()?.join(".dev").join("config.toml"))
            .map_err(|_| anyhow!("global config path must be valid UTF-8"))?
    } else {
        Utf8PathBuf::from_path_buf(cwd.join(".dev").join("config.toml"))
            .map_err(|_| anyhow!("local config path must be valid UTF-8"))?
    };

    let config = render_config(&languages);
    write_text_file(ctx, &config_path, &config, args.force, !args.yes)?;
    println!("Wrote config: {}", config_path);

    if write_os_configs {
        for name in ["linux", "windows", "macos"] {
            let path = config_path
                .parent()
                .expect("config path has parent")
                .join(format!("config.{name}.toml"));
            write_text_file(ctx, &path, &config, args.force, !args.yes)?;
            println!("Wrote OS config: {}", path);
        }
    }

    if !args.global {
        scaffold_project_files(ctx, &root, &languages, args.force, !args.yes)?;
    }

    if write_ci {
        bootstrap_dev_binary(ctx, &root, args.force, !args.yes)?;
        let runner = choose_runner(&theme, &args)?;
        let workflow = render_ci_workflow(&runner);
        let workflow_path = Utf8PathBuf::from_path_buf(root.join(".github/workflows/dev-ci.yml"))
            .map_err(|_| anyhow!("workflow path must be valid UTF-8"))?;
        write_text_file(ctx, &workflow_path, &workflow, args.force, !args.yes)?;
        println!("Wrote CI workflow: {}", workflow_path);
    }

    if install_tooling {
        install_selected_tooling(ctx, &languages, write_ci)?;
    }

    println!("dev init complete");
    Ok(())
}

fn select_languages(
    theme: &ColorfulTheme,
    args: &InitArgs,
    detected: &[Language],
) -> Result<Vec<Language>> {
    if !args.languages.is_empty() {
        let mut selected = Vec::new();
        for value in &args.languages {
            selected.push(parse_language(value)?);
        }
        return Ok(dedupe_languages(selected));
    }

    if args.yes {
        return Ok(if let Some(first) = detected.first() {
            vec![first.clone()]
        } else {
            vec![Language::Rust]
        });
    }

    let all = [Language::Rust, Language::Python, Language::TypeScript];
    let defaults: Vec<bool> = all
        .iter()
        .map(|language| detected.contains(language))
        .collect();
    let defaults = if defaults.iter().any(|selected| *selected) {
        defaults
    } else {
        vec![true, false, false]
    };
    let labels: Vec<&str> = all.iter().map(Language::label).collect();
    let picks = MultiSelect::with_theme(theme)
        .with_prompt("Select project languages")
        .items(&labels)
        .defaults(&defaults)
        .interact()?;

    Ok(picks.into_iter().map(|idx| all[idx].clone()).collect())
}

fn choose_runner(theme: &ColorfulTheme, args: &InitArgs) -> Result<String> {
    if let Some(runner) = &args.runner {
        return Ok(runner.clone());
    }
    if args.yes {
        return Ok("ubuntu-latest".to_owned());
    }

    let labels = [
        "ubuntu-latest",
        "self-hosted linux x64",
        "self-hosted linux arm64",
    ];
    let idx = Select::with_theme(theme)
        .with_prompt("GitHub Actions runner")
        .items(&labels)
        .default(0)
        .interact()?;
    Ok(match idx {
        1 => "self-hosted, linux, x64".to_owned(),
        2 => "self-hosted, linux, arm64".to_owned(),
        _ => "ubuntu-latest".to_owned(),
    })
}

fn parse_language(value: &str) -> Result<Language> {
    match value.trim().to_ascii_lowercase().as_str() {
        "rust" | "rs" => Ok(Language::Rust),
        "python" | "py" => Ok(Language::Python),
        "typescript" | "ts" | "node" | "javascript" | "js" => Ok(Language::TypeScript),
        other => bail!("unsupported language `{other}`"),
    }
}

fn dedupe_languages(languages: Vec<Language>) -> Vec<Language> {
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for language in languages {
        if seen.insert(language.as_str()) {
            out.push(language);
        }
    }
    out
}

fn detect_languages(root: &Path) -> Vec<Language> {
    let mut languages = Vec::new();
    if root.join("Cargo.toml").is_file() {
        languages.push(Language::Rust);
    }
    if root.join("pyproject.toml").is_file()
        || root.join("requirements.txt").is_file()
        || root.join("setup.py").is_file()
    {
        languages.push(Language::Python);
    }
    if root.join("package.json").is_file() {
        languages.push(Language::TypeScript);
    }
    languages
}

fn scaffold_project_files(
    ctx: &CliContext,
    root: &Path,
    languages: &[Language],
    force: bool,
    interactive: bool,
) -> Result<()> {
    write_combined_gitignore(ctx, root, languages, force, interactive)?;

    for language in languages {
        match language {
            Language::Rust => {
                write_template_if_missing(
                    ctx,
                    root.join(".cargo/config.toml"),
                    "rust/cargo-config.toml",
                    force,
                    interactive,
                )?;
            }
            Language::Python => {
                write_template_if_missing(
                    ctx,
                    root.join("pyproject.toml"),
                    "python/pyproject.toml",
                    force,
                    interactive,
                )?;
            }
            Language::TypeScript => {
                for (path, template) in [
                    ("package.json", "typescript/package.json"),
                    ("eslint.config.ts", "typescript/eslint.config.ts"),
                    ("tsconfig.json", "typescript/tsconfig.json"),
                    ("vitest.config.ts", "typescript/vitest.config.ts"),
                ] {
                    write_template_if_missing(ctx, root.join(path), template, force, interactive)?;
                }
            }
        }
    }
    Ok(())
}

fn bootstrap_dev_binary(
    ctx: &CliContext,
    root: &Path,
    force: bool,
    interactive: bool,
) -> Result<()> {
    let current_exe = std::env::current_exe().context("locating current dev executable")?;
    let bin_dir = root.join(".dev/bin");
    let dev_path = Utf8PathBuf::from_path_buf(bin_dir.join("dev"))
        .map_err(|_| anyhow!("dev binary path must be valid UTF-8"))?;

    if dev_path.exists() && !force {
        if interactive {
            let overwrite = Confirm::with_theme(&ColorfulTheme::default())
                .with_prompt(format!("{} exists. Overwrite?", dev_path))
                .default(false)
                .interact()?;
            if !overwrite {
                println!("Skipped existing file: {}", dev_path);
            } else {
                copy_binary(ctx, &current_exe, &dev_path)?;
            }
        } else {
            println!(
                "Skipped existing file: {} (use --force to overwrite)",
                dev_path
            );
        }
    } else {
        copy_binary(ctx, &current_exe, &dev_path)?;
    }

    write_sha256sums(ctx, &bin_dir)?;
    let attrs = Utf8PathBuf::from_path_buf(root.join(".gitattributes"))
        .map_err(|_| anyhow!(".gitattributes path must be valid UTF-8"))?;
    write_text_file(
        ctx,
        &attrs,
        ".dev/bin/dev* filter=lfs diff=lfs merge=lfs -text\n",
        force,
        interactive,
    )?;
    Ok(())
}

fn copy_binary(ctx: &CliContext, source: &Path, destination: &Utf8Path) -> Result<()> {
    if ctx.dry_run {
        println!("[dry-run] copy {} -> {}", source.display(), destination);
        return Ok(());
    }

    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).with_context(|| format!("creating directory {}", parent))?;
    }
    fs::copy(source, destination)
        .with_context(|| format!("copying {} to {}", source.display(), destination))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(destination.as_std_path())
            .with_context(|| format!("reading metadata for {}", destination))?
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(destination.as_std_path(), perms)
            .with_context(|| format!("setting executable permissions on {}", destination))?;
    }

    Ok(())
}

fn write_sha256sums(ctx: &CliContext, bin_dir: &Path) -> Result<()> {
    let sums_path = Utf8PathBuf::from_path_buf(bin_dir.join("SHA256SUMS"))
        .map_err(|_| anyhow!("SHA256SUMS path must be valid UTF-8"))?;
    if ctx.dry_run {
        println!("[dry-run] write {}", sums_path);
        return Ok(());
    }

    fs::create_dir_all(bin_dir)
        .with_context(|| format!("creating directory {}", bin_dir.display()))?;
    let mut entries = Vec::new();
    for entry in fs::read_dir(bin_dir).with_context(|| format!("reading {}", bin_dir.display()))? {
        let entry = entry?;
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if !name.starts_with("dev") || !path.is_file() {
            continue;
        }
        let bytes = fs::read(&path).with_context(|| format!("reading {}", path.display()))?;
        let digest = Sha256::digest(&bytes);
        entries.push(format!("{:x}  {}", digest, name));
    }
    entries.sort();

    let mut file = fs::File::create(sums_path.as_std_path())
        .with_context(|| format!("writing {}", sums_path))?;
    for entry in entries {
        writeln!(file, "{entry}")?;
    }
    Ok(())
}

fn write_combined_gitignore(
    ctx: &CliContext,
    root: &Path,
    languages: &[Language],
    force: bool,
    interactive: bool,
) -> Result<()> {
    let mut sections = Vec::new();
    for language in languages {
        let template = match language {
            Language::Rust => "rust/.gitignore",
            Language::Python => "python/.gitignore",
            Language::TypeScript => "typescript/.gitignore",
        };
        sections.push(templates::get_string(template)?);
    }

    let mut content = String::new();
    let mut seen = BTreeSet::new();
    for line in sections
        .iter()
        .flat_map(|section| section.lines())
        .map(str::trim_end)
    {
        if line.is_empty() {
            continue;
        }
        if seen.insert(line.to_owned()) {
            content.push_str(line);
            content.push('\n');
        }
    }

    write_text_file(
        ctx,
        &Utf8PathBuf::from_path_buf(root.join(".gitignore"))
            .map_err(|_| anyhow!(".gitignore path must be valid UTF-8"))?,
        &content,
        force,
        interactive,
    )
}

fn write_template_if_missing(
    ctx: &CliContext,
    destination: PathBuf,
    template: &str,
    force: bool,
    interactive: bool,
) -> Result<()> {
    let destination = Utf8PathBuf::from_path_buf(destination)
        .map_err(|_| anyhow!("destination path must be valid UTF-8"))?;
    let content = templates::get_string(template)?;
    write_text_file(ctx, &destination, &content, force, interactive)
}

fn write_text_file(
    ctx: &CliContext,
    path: &Utf8Path,
    content: &str,
    force: bool,
    interactive: bool,
) -> Result<()> {
    if path.exists() && !force {
        if interactive {
            let overwrite = Confirm::with_theme(&ColorfulTheme::default())
                .with_prompt(format!("{} exists. Overwrite?", path))
                .default(false)
                .interact()?;
            if !overwrite {
                println!("Skipped existing file: {}", path);
                return Ok(());
            }
        } else {
            println!("Skipped existing file: {} (use --force to overwrite)", path);
            return Ok(());
        }
    }

    if ctx.dry_run {
        println!("[dry-run] write {}", path);
        return Ok(());
    }

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("creating directory {}", parent))?;
    }
    fs::write(path, content).with_context(|| format!("writing {}", path))
}

fn install_selected_tooling(
    ctx: &CliContext,
    languages: &[Language],
    include_ci: bool,
) -> Result<()> {
    let mut components = Vec::new();
    if include_ci {
        components.push(Component::GitLfs);
    }
    for language in languages {
        match language {
            Language::Rust => components.push(Component::Rustup),
            Language::Python => components.push(Component::Uv),
            Language::TypeScript => {
                components.push(Component::Node);
                components.push(Component::Pnpm);
            }
        }
    }
    components.sort_by_key(Component::name);
    components.dedup();

    if components.is_empty() {
        return Ok(());
    }

    let log_file = home_dir()?.join(".dev").join("setup.log");
    let setup_ctx = SetupContext::new(ctx.dry_run, Some(log_file), SetupConfig::default())?;
    crate::setup::run_setup(&setup_ctx, components, true, false)
}

fn render_config(languages: &[Language]) -> String {
    let default_language = languages.first().map(Language::as_str).unwrap_or("rust");
    let mut out = format!(
        r#"# dev configuration file

default_language = "{default_language}"

"#
    );

    if languages.contains(&Language::Rust) {
        out.push_str(RUST_CONFIG);
    }
    if languages.contains(&Language::Python) {
        out.push_str(PYTHON_CONFIG);
    }
    if languages.contains(&Language::TypeScript) {
        out.push_str(TYPESCRIPT_CONFIG);
    }

    push_all_task(&mut out, "fmt", languages);
    push_all_task(&mut out, "lint", languages);
    push_all_task(&mut out, "type", languages);
    push_all_task(&mut out, "test", languages);
    push_all_task(&mut out, "fix", languages);
    push_all_task(&mut out, "check", languages);
    push_all_task(&mut out, "ci", languages);
    out.push_str(GIT_ENV_CONFIG);
    out
}

fn push_all_task(out: &mut String, verb: &str, languages: &[Language]) {
    let tasks: Vec<String> = languages
        .iter()
        .map(|language| {
            let prefix = match language {
                Language::Rust => "rust",
                Language::Python => "py",
                Language::TypeScript => "ts",
            };
            format!("\"{prefix}_{verb}\"")
        })
        .collect();
    out.push_str(&format!(
        "\n[tasks.all_{verb}]\ncommands = [{}]\n",
        tasks.join(", ")
    ));
}

fn render_ci_workflow(runner: &str) -> String {
    format!(
        r#"name: dev ci

on:
  push:
  pull_request:

jobs:
  dev-ci:
    runs-on: [{runner}]
    steps:
      - uses: actions/checkout@v4
        with:
          lfs: true
      - name: Ensure dev binary is executable
        run: chmod +x .dev/bin/dev
      - name: Run dev ci
        run: ./.dev/bin/dev ci
"#
    )
}

fn env_current_dir() -> Result<PathBuf> {
    std::env::current_dir().context("determining current directory")
}

fn home_dir() -> Result<PathBuf> {
    dirs::home_dir().ok_or_else(|| anyhow!("unable to determine home directory"))
}

const RUST_CONFIG: &str = r#"# ===================== Rust ========================

[tasks.rust_fmt]
commands = [["cargo", "fmt"]]

[tasks.rust_fmt_check]
commands = [["cargo", "fmt", "--", "--check"]]

[tasks.rust_lint]
commands = [["cargo", "clippy", "--", "-D", "warnings"]]

[tasks.rust_type]
commands = [["cargo", "check"]]

[tasks.rust_test]
commands = [["cargo", "test"]]

[tasks.rust_fix]
commands = ["rust_fmt"]

[tasks.rust_check]
commands = ["rust_fmt_check", "rust_lint", "rust_test"]

[tasks.rust_ci]
commands = ["rust_check"]

[languages.rust]
install = []

[languages.rust.pipelines]
fmt = ["rust_fmt"]
lint = ["rust_lint"]
type = ["rust_type"]
test = ["rust_test"]
fix = ["rust_fix"]
check = ["rust_check"]
ci = ["rust_ci"]

"#;

const PYTHON_CONFIG: &str = r#"# ===================== Python ========================

[tasks.py_fmt]
commands = [["uv", "run", "ruff", "format", "."]]

[tasks.py_fmt_check]
commands = [["uv", "run", "ruff", "format", "--check", "."]]

[tasks.py_lint]
commands = [["uv", "run", "ruff", "check", "."]]

[tasks.py_lint_fix]
commands = [["uv", "run", "ruff", "check", ".", "--fix"]]

[tasks.py_type]
commands = [["uv", "run", "mypy", "."]]

[tasks.py_test]
commands = [["uv", "run", "pytest"]]

[tasks.py_fix]
commands = ["py_lint_fix", "py_fmt"]

[tasks.py_check]
commands = ["py_fmt_check", "py_lint", "py_test"]

[tasks.py_ci]
commands = ["py_check"]

[languages.python]
install = [["uv", "sync"]]

[languages.python.pipelines]
fmt = ["py_fmt"]
lint = ["py_lint"]
type = ["py_type"]
test = ["py_test"]
fix = ["py_fix"]
check = ["py_check"]
ci = ["py_ci"]

"#;

const TYPESCRIPT_CONFIG: &str = r#"# ===================== TypeScript ========================

[tasks.ts_fmt]
commands = [["pnpm", "prettier", "--write", "."]]

[tasks.ts_fmt_check]
commands = [["pnpm", "prettier", "--check", "."]]

[tasks.ts_lint]
commands = [["pnpm", "eslint", ".", "--ext", ".ts,.tsx", "--max-warnings", "0"]]

[tasks.ts_lint_fix]
commands = [["pnpm", "eslint", ".", "--ext", ".ts,.tsx", "--fix"]]

[tasks.ts_type]
commands = [["pnpm", "tsc", "--noEmit"]]

[tasks.ts_test]
commands = [["pnpm", "vitest", "--run"]]

[tasks.ts_fix]
commands = ["ts_lint_fix", "ts_fmt"]

[tasks.ts_check]
commands = ["ts_fmt_check", "ts_lint", "ts_test"]

[tasks.ts_ci]
commands = ["ts_check"]

[languages.typescript]
install = [["pnpm", "install"]]

[languages.typescript.pipelines]
fmt = ["ts_fmt"]
lint = ["ts_lint"]
type = ["ts_type"]
test = ["ts_test"]
fix = ["ts_fix"]
check = ["ts_check"]
ci = ["ts_ci"]

"#;

const GIT_ENV_CONFIG: &str = r#"
# ===================== Git ========================

[git]
# main_branch = "main"
# release_branch = "release-candidate"

# ===================== Environment ========================

[env]
# required = ["DATABASE_URL"]
# optional = ["LOG_LEVEL"]
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_config_keeps_single_language_configs_small() {
        let config = render_config(&[Language::Rust]);

        assert!(config.contains("default_language = \"rust\""));
        assert!(config.contains("[tasks.rust_ci]"));
        assert!(config.contains("commands = [\"rust_ci\"]"));
        assert!(!config.contains("[tasks.py_ci]"));
        assert!(!config.contains("[tasks.ts_ci]"));
    }

    #[test]
    fn render_config_aggregates_selected_polyglot_languages() {
        let config = render_config(&[Language::Rust, Language::Python, Language::TypeScript]);

        assert!(config.contains("[tasks.all_ci]"));
        assert!(config.contains("commands = [\"rust_ci\", \"py_ci\", \"ts_ci\"]"));
        assert!(config.contains("[languages.python.pipelines]"));
        assert!(config.contains("[languages.typescript.pipelines]"));
    }
}
