use std::path::{Path, PathBuf};

use anyhow::Result;
use clap::{Args, Parser, Subcommand, ValueEnum, error::ErrorKind};

use crate::config;

/// Top-level CLI definition matching the spec in `docs/spec.md`.
#[derive(Parser, Debug)]
#[command(name = "dev", version, about = "Unified developer workflows")]
pub struct Cli {
    #[arg(short = 'C', long = "chdir")]
    pub chdir: Option<PathBuf>,
    #[arg(short = 'f', long = "file")]
    pub file: Option<PathBuf>,
    #[arg(long = "project", global = true)]
    pub project: Option<String>,
    #[arg(short = 'l', long = "language")]
    pub language: Option<String>,
    #[arg(short = 'n', long = "dry-run", global = true)]
    pub dry_run: bool,
    #[arg(short = 'v', long = "verbose", action = clap::ArgAction::Count)]
    pub verbose: u8,
    #[arg(long = "no-color", global = true)]
    pub no_color: bool,
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// List available tasks and pipelines.
    List,
    /// Execute a named task or pipeline.
    Run { task: String },
    /// Start a long-running development server for the current project.
    Start(StartArgs),
    /// Standard verbs dispatch to the current or selected language pipeline.
    #[command(hide = true)]
    Fmt,
    #[command(hide = true)]
    Lint,
    #[command(name = "type")]
    #[command(hide = true)]
    TypeCheck,
    #[command(hide = true)]
    Test,
    #[command(hide = true)]
    Fix,
    #[command(hide = true)]
    Check,
    #[command(hide = true)]
    Ci,
    /// Run aggregations across all languages for a given verb.
    All { verb: Verb },
    /// Install tooling and scaffolds for a language (defaults to configured language).
    Install(InstallArgs),
    /// Manage language defaults.
    Language {
        #[command(subcommand)]
        command: LanguageCommand,
    },
    /// Git-centric flows such as branch management and release PRs.
    Git {
        #[command(subcommand)]
        command: GitCommand,
    },
    /// Version bumping, changelog, and tagging.
    Version {
        #[command(subcommand)]
        command: VersionCommand,
    },
    /// Environment variable helper commands backed by a `.env` file.
    Env(EnvArgs),
    /// Configuration display, validation, and template generation.
    Config {
        #[command(subcommand)]
        command: Option<ConfigCommand>,
    },
    /// System setup and installation management.
    Setup {
        #[command(subcommand)]
        command: Option<SetupCommand>,
        /// Skip components that are already installed
        #[arg(long = "skip-installed", global = true)]
        skip_installed: bool,
        /// Don't auto-install dependencies
        #[arg(long = "no-deps", global = true)]
        no_deps: bool,
    },
    /// Generate a Markdown code review overlay from git diffs.
    Review {
        /// Path to the markdown file to write
        #[arg(long = "output")]
        output: Option<PathBuf>,
        /// Include unstaged working tree changes in the report
        #[arg(long = "include-working")]
        include_working: bool,
        /// Compare current branch against main instead of showing staged changes
        #[arg(long = "main")]
        main: bool,
    },
    /// Generate a directory structure map with file contents (for LLM context).
    Walk {
        /// Directory to map (default: current directory)
        #[arg(default_value = ".")]
        directory: PathBuf,
        /// Output file path (default: manifest.md)
        #[arg(short = 'o', long = "output", default_value = "manifest.md")]
        output: PathBuf,
        /// Output format
        #[arg(long = "format", default_value = "markdown")]
        format: String,
        /// Maximum depth to traverse
        #[arg(long = "max-depth", default_value = "10")]
        max_depth: u32,
        /// Exclude file contents (include by default)
        #[arg(long = "no-content")]
        no_content: bool,
        /// File extensions to include content from (e.g., .rs .py .ts)
        #[arg(long = "extensions", num_args = 1..)]
        extensions: Option<Vec<String>>,
        /// Include hidden files
        #[arg(long = "include-hidden")]
        include_hidden: bool,
    },
    /// Run commands in a shell and print a compact execution summary.
    Summary {
        #[command(subcommand)]
        command: SummaryCommand,
    },
    /// Launch configured coding agents with a prompt and model.
    Agent {
        #[command(subcommand)]
        command: AgentCommand,
    },
    /// Docker helpers for generating base/project containers.
    Docker {
        #[command(subcommand)]
        command: DockerCommand,
    },
    /// Internal research project scaffolding.
    #[command(hide = true)]
    Research {
        #[command(subcommand)]
        command: ResearchCommand,
    },
    /// 1Password vault operations for managing secrets.
    Vault {
        #[command(subcommand)]
        command: VaultCommand,
    },
    /// Write platform-specific config overrides for the target host OS.
    Os {
        #[command(subcommand)]
        command: OsCommand,
    },
    #[command(external_subcommand)]
    External(Vec<String>),
}

#[derive(Subcommand, Debug)]
pub enum ResearchCommand {
    /// Scaffold an isolated, project-local research workspace.
    Init(ResearchInitArgs),
}

#[derive(Subcommand, Debug)]
pub enum SummaryCommand {
    /// Run a configured task and summarize captured output.
    Run(SummaryRunArgs),
    /// Run an ad-hoc command and summarize captured output.
    Exec(SummaryExecArgs),
}

#[derive(Subcommand, Debug)]
pub enum AgentCommand {
    /// Run a configured agent adapter.
    Run(AgentRunArgs),
    /// Show known background agent jobs.
    List,
    /// Show status and a compact log summary for a background agent job.
    Status(AgentStatusArgs),
}

#[derive(Args, Debug)]
pub struct AgentRunArgs {
    /// Agent config name. Defaults to `default` or the configured default agent.
    #[arg(default_value = "default")]
    pub agent: String,

    /// Prompt text to send to the agent.
    #[arg(long = "prompt", short = 'p')]
    pub prompt: Option<String>,

    /// Read prompt text from a file. Use `-` to read stdin.
    #[arg(long = "prompt-file")]
    pub prompt_file: Option<PathBuf>,

    /// Override the configured model.
    #[arg(long = "model", short = 'm')]
    pub model: Option<String>,

    /// Override the configured working directory.
    #[arg(long = "cwd", short = 'C')]
    pub cwd: Option<PathBuf>,

    /// Run in the foreground instead of launching an async job.
    #[arg(long = "attach", short = 'a', default_value_t = false)]
    pub attach: bool,

    /// Override the configured loop iteration count.
    #[arg(long = "iterations", short = 'i')]
    pub iterations: Option<u32>,

    /// Additional adapter arguments.
    #[arg(long = "arg")]
    pub extra_args: Vec<String>,
}

#[derive(Args, Debug)]
pub struct AgentStatusArgs {
    /// Job id printed by async `dev agent run`.
    pub job_id: String,

    /// Number of log lines to include.
    #[arg(long = "tail", default_value_t = 80)]
    pub tail: usize,
}

#[derive(Args, Debug)]
pub struct SummaryRunArgs {
    /// Configured task name to run.
    pub task: String,

    /// Print the raw captured output after the summary.
    #[arg(long = "raw", default_value_t = false)]
    pub raw: bool,
}

#[derive(Args, Debug)]
pub struct SummaryExecArgs {
    /// Print the raw captured output after the summary.
    #[arg(long = "raw", default_value_t = false)]
    pub raw: bool,

    /// Command argv to execute after `--`.
    #[arg(trailing_var_arg = true, required = true)]
    pub argv: Vec<String>,
}

#[derive(Args, Debug)]
pub struct ResearchInitArgs {
    /// Target directory for the research project (default: current directory).
    #[arg(default_value = ".")]
    pub directory: PathBuf,

    /// Project name written into project.yaml (default: directory name).
    #[arg(long = "name")]
    pub name: Option<String>,

    /// Python package name for reusable bindings/logic.
    #[arg(long = "package")]
    pub package: Option<String>,

    /// Overwrite scaffold files if they already exist.
    #[arg(long = "force", default_value_t = false)]
    pub force: bool,

    /// Skip automatic harness dependency install with uv.
    #[arg(long = "skip-install", default_value_t = false)]
    pub skip_install: bool,

    /// Git URL used to install research-harness.
    #[arg(
        long = "harness-git",
        default_value = "https://github.com/hydra-dynamix/research-harness-2.git"
    )]
    pub harness_git: String,
}

#[derive(Subcommand, Debug)]
pub enum OsCommand {
    /// Write Windows-oriented config overrides (uses npx.cmd, .cmd extensions).
    Windows,
    /// Write Linux/macOS-oriented config overrides (uses npx, no extensions).
    Linux,
    /// Show the detected host OS and the active override target.
    Show,
}

#[derive(Subcommand, Debug)]
pub enum DockerCommand {
    /// Generate docker/Dockerfile.core, docker-compose.yml, and .env for the current project.
    Init(DockerInitArgs),
    /// Build docker/Dockerfile.core into the configured CORE_IMAGE tag.
    Build(DockerBuildArgs),
    /// Docker compose helpers.
    Compose {
        #[command(subcommand)]
        command: DockerComposeCommand,
    },
    /// Start the compose service (build if needed) and open an interactive shell inside it.
    #[command(alias = "dev")]
    Develop(DockerDevelopArgs),
}

#[derive(Args, Debug)]
pub struct DockerDevelopArgs {
    /// Compose service name (default: core)
    #[arg(long = "service", default_value = "core")]
    pub service: String,

    /// Skip `docker compose up -d --build` and only exec a shell
    #[arg(long = "no-up", default_value_t = false)]
    pub no_up: bool,
}

#[derive(Args, Debug)]
pub struct DockerBuildArgs {
    /// Override the tag to build (defaults to CORE_IMAGE from .env)
    #[arg(long = "image")]
    pub image: Option<String>,
}

#[derive(Subcommand, Debug)]
pub enum DockerComposeCommand {
    Up {
        #[command(subcommand)]
        command: DockerComposeUpCommand,
    },
}

#[derive(Subcommand, Debug)]
pub enum DockerComposeUpCommand {
    /// Run `docker compose up --build`
    Build(DockerComposeUpBuildArgs),
}

#[derive(Args, Debug)]
pub struct DockerComposeUpBuildArgs {
    /// Run in the background
    #[arg(short = 'd', long = "detach", default_value_t = false)]
    pub detach: bool,
}

#[derive(Args, Debug)]
pub struct DockerInitArgs {
    /// Overwrite existing files
    #[arg(long = "force", default_value_t = false)]
    pub force: bool,

    /// Base image to use in docker/Dockerfile.core
    #[arg(
        long = "base-image",
        default_value = "nvcr.io/nvidia/pytorch:25.09-py3"
    )]
    pub base_image: String,

    /// Compose service name (default: core)
    #[arg(long = "service", default_value = "core")]
    pub service: String,
}

/// Shared verb enumeration for consistent handling across languages.
#[derive(ValueEnum, Clone, Copy, Debug, Eq, PartialEq)]
pub enum Verb {
    Fmt,
    Lint,
    #[value(name = "type")]
    TypeCheck,
    Test,
    Fix,
    Check,
    Ci,
}

impl Verb {
    pub fn as_str(&self) -> &'static str {
        match self {
            Verb::Fmt => "fmt",
            Verb::Lint => "lint",
            Verb::TypeCheck => "type",
            Verb::Test => "test",
            Verb::Fix => "fix",
            Verb::Check => "check",
            Verb::Ci => "ci",
        }
    }
}

#[derive(Subcommand, Debug)]
pub enum LanguageCommand {
    /// Set the global default language in the user config.
    Set { name: String },
}

#[derive(Args, Debug)]
pub struct InstallArgs {
    #[arg()]
    pub language: Option<String>,
    /// Overwrite existing scaffold files instead of skipping them.
    #[arg(long = "force", default_value_t = false)]
    pub force: bool,
    /// Install tooling and run provisioning commands without writing scaffold files.
    #[arg(long = "no-scaffold", default_value_t = false)]
    pub no_scaffold: bool,
}

#[derive(Args, Debug)]
pub struct StartArgs {
    /// Override the default port for the start command.
    #[arg(long = "port")]
    pub port: Option<u16>,

    /// Use the production port default (8091) instead of the development default (8031).
    #[arg(long = "prod", default_value_t = false)]
    pub prod: bool,
}

#[derive(Subcommand, Debug)]
pub enum GitCommand {
    BranchCreate(BranchCreate),
    BranchFinalize(BranchFinalize),
    ReleasePr(ReleasePr),
}

#[derive(Args, Debug)]
pub struct BranchCreate {
    pub name: String,
    #[arg(long = "from")]
    pub base: Option<String>,
    #[arg(long)]
    pub push: bool,
    #[arg(long = "allow-dirty")]
    pub allow_dirty: bool,
}

#[derive(Args, Debug)]
pub struct BranchFinalize {
    #[arg()]
    pub name: Option<String>,
    #[arg(long = "into")]
    pub base: Option<String>,
    #[arg(long)]
    pub delete: bool,
    #[arg(long = "allow-dirty")]
    pub allow_dirty: bool,
}

#[derive(Args, Debug)]
pub struct ReleasePr {
    /// Version bump level (major, minor, patch, prerelease)
    #[arg(value_enum)]
    pub bump: BumpLevel,
    #[arg(long = "from")]
    pub from: Option<String>,
    #[arg(long = "to")]
    pub to: Option<String>,
    #[arg(long = "no-open")]
    pub no_open: bool,
}

#[derive(Subcommand, Debug)]
pub enum VersionCommand {
    Bump(VersionBump),
    Changelog(ChangelogArgs),
    Show,
}

#[derive(Args, Debug)]
pub struct VersionBump {
    #[arg(value_enum)]
    pub level: BumpLevel,
    #[arg(long = "custom")]
    pub custom: Option<String>,
    #[arg(long = "tag")]
    pub tag: bool,
    #[arg(long = "no-commit")]
    pub no_commit: bool,
    #[arg(long = "no-changelog")]
    pub no_changelog: bool,
}

#[derive(Copy, Clone, Debug, ValueEnum)]
pub enum BumpLevel {
    Major,
    Minor,
    Patch,
    Prerelease,
}

#[derive(Args, Debug)]
pub struct ChangelogArgs {
    #[arg(long = "since")]
    pub since: Option<String>,
    #[arg(long = "unreleased")]
    pub unreleased: bool,
}

#[derive(Args, Debug)]
pub struct EnvArgs {
    /// Show values unmasked
    #[arg(long = "raw", default_value_t = false)]
    pub raw: bool,

    #[command(subcommand)]
    pub command: Option<EnvCommand>,
}

#[derive(Subcommand, Debug)]
pub enum EnvCommand {
    /// List all environment variables (default if no subcommand)
    List,
    /// Get a single environment variable value
    Get { key: String },
    /// Add or update an environment variable
    Add { key: String, value: String },
    /// Remove an environment variable
    Rm { key: String },
    /// List available environment profiles (.env.*)
    Profiles,
    /// Switch to a different environment profile
    Switch { profile: String },
    /// Save current .env as a named profile
    Save { name: String },
    /// Validate .env against required keys in config
    Check,
    /// Initialize .env from .env.example if missing
    Init,
    /// Generate .env.example from current .env (values stripped)
    Template,
    /// Show diff between .env and a reference file
    Diff {
        /// Reference file to compare against (default: .env.example)
        #[arg(default_value = ".env.example")]
        reference: String,
    },
    /// Interactively add missing keys from a reference file
    Sync {
        /// Reference file to sync from (default: .env.example)
        #[arg(default_value = ".env.example")]
        reference: String,
    },
}

#[derive(Subcommand, Debug)]
pub enum ConfigCommand {
    Show,
    Path,
    Check,
    Generate {
        #[arg()]
        path: Option<PathBuf>,
        #[arg(long = "force", default_value_t = false)]
        force: bool,
    },
    Reload,
    Add {
        #[arg()]
        name: Option<String>,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        command: Vec<String>,
        #[arg(long = "force", default_value_t = false)]
        force: bool,
        #[arg(long = "append", default_value_t = false)]
        append: bool,
    },
}

#[derive(Subcommand, Debug)]
pub enum SetupCommand {
    /// Run default components with --skip-installed implied
    Run {
        /// Components to install
        components: Vec<String>,
        #[arg(long = "skip-installed")]
        skip_installed: bool,
        #[arg(long = "no-deps")]
        no_deps: bool,
    },
    Inference {
        #[arg()]
        service: String,
        #[arg(long = "dest")]
        dest: Option<PathBuf>,
        #[arg(long = "force", default_value_t = false)]
        force: bool,
        #[arg(long = "no-cache", default_value_t = false)]
        no_cache: bool,
    },
    /// Run all compatible components
    All {
        #[arg(long = "skip-installed")]
        skip_installed: bool,
        #[arg(long = "no-deps")]
        no_deps: bool,
    },
    /// Show installation status of all components
    Status,
    /// List available components and their dependencies
    List,
    /// Show effective setup configuration
    Config,
}

#[derive(Subcommand, Debug)]
pub enum VaultCommand {
    /// List secrets in a vault
    List {
        /// Vault to list from (production or development)
        #[arg(long = "account", default_value = "development")]
        account: String,
    },
    /// Get a secret value
    Get {
        /// Secret name or path
        item: String,
        /// Specific field to extract
        #[arg(long = "field")]
        field: Option<String>,
        /// Vault account (production or development)
        #[arg(long = "account", default_value = "development")]
        account: String,
    },
    /// Create or update a secret
    Set {
        /// Secret name
        item: String,
        /// Secret value
        value: String,
        /// Vault account (production or development)
        #[arg(long = "account", default_value = "development")]
        account: String,
    },
    /// Delete a secret
    Delete {
        /// Secret name
        item: String,
        /// Vault account (production or development)
        #[arg(long = "account", default_value = "development")]
        account: String,
    },
}

/// Helper entry point so `main` can stay minimal.
pub fn parse() -> Result<Cli> {
    let args = std::env::args_os().collect::<Vec<_>>();
    match Cli::try_parse_from(&args) {
        Ok(cli) => Ok(cli),
        Err(err)
            if matches!(
                err.kind(),
                ErrorKind::DisplayHelp | ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
            ) =>
        {
            print!("{err}");
            if should_append_dynamic_help(&args)
                && let Some(dynamic) = dynamic_help(&args)?
            {
                print!("{dynamic}");
            }
            std::process::exit(0);
        }
        Err(err) if err.kind() == ErrorKind::DisplayVersion => {
            print!("{err}");
            std::process::exit(0);
        }
        Err(err) => Err(anyhow::anyhow!(err.to_string())),
    }
}

fn should_append_dynamic_help(args: &[std::ffi::OsString]) -> bool {
    let mut saw_help = false;
    let mut positional = Vec::new();
    let mut skip_next = false;

    for arg in args.iter().skip(1) {
        if skip_next {
            skip_next = false;
            continue;
        }

        let value = arg.to_string_lossy();
        match value.as_ref() {
            "--help" | "-h" => {
                saw_help = true;
                continue;
            }
            "--file" | "-f" | "--chdir" | "-C" | "--project" | "--language" | "-l" => {
                skip_next = true;
                continue;
            }
            _ => {}
        }

        if value.starts_with("--file=")
            || value.starts_with("--chdir=")
            || value.starts_with("--project=")
            || value.starts_with("--language=")
            || value.starts_with('-')
        {
            continue;
        }

        positional.push(value.into_owned());
    }

    saw_help && positional.is_empty()
}

fn dynamic_help(args: &[std::ffi::OsString]) -> Result<Option<String>> {
    let Some(path) = resolve_help_config_path(args) else {
        return Ok(None);
    };
    if !path.exists() {
        return Ok(None);
    }

    let path = camino::Utf8PathBuf::from_path_buf(path)
        .map_err(|_| anyhow::anyhow!("config path must be valid UTF-8"))?;
    let cfg = match config::load_from_path(&path) {
        Ok(cfg) => cfg,
        Err(err) => {
            return Ok(Some(format!(
                "\nConfigured items unavailable\n  Could not load config {path}: {err:#}\n"
            )));
        }
    };
    let mut out = String::new();
    let mut wrote_any = false;

    let task_count = cfg.tasks.as_ref().map(|tasks| tasks.len()).unwrap_or(0);
    let language_summary = summarize_languages(&cfg);
    let agent_summary = summarize_agents(&cfg);

    if task_count > 0 || !language_summary.is_empty() || !agent_summary.is_empty() {
        wrote_any = true;
        out.push_str("\nConfigured workflows\n");
        out.push_str("  verbs: dev fmt|lint|type|test|fix|check|ci\n");
        if task_count > 0 {
            out.push_str(&format!(
                "  tasks: {task_count} configured (run `dev list` for names)\n"
            ));
            if let Some(example) = first_task_name(&cfg) {
                out.push_str(&format!("  example: dev run {example}\n"));
            }
        }
        if !language_summary.is_empty() {
            out.push_str(&format!("  languages: {}\n", language_summary.join("; ")));
        }
        if !agent_summary.is_empty() {
            out.push_str(&format!("  agents: {}\n", agent_summary.join("; ")));
        }
        out.push_str("  details: dev list | dev config show | dev agent list\n");
    }

    if wrote_any {
        out.push_str(&format!("\nConfig source: {path}\n"));
        Ok(Some(out))
    } else {
        Ok(None)
    }
}

fn resolve_help_config_path(args: &[std::ffi::OsString]) -> Option<PathBuf> {
    let mut iter = args.iter().peekable();
    while let Some(arg) = iter.next() {
        let value = arg.to_string_lossy();
        if value == "--file" || value == "-f" {
            return iter.next().map(PathBuf::from);
        }
        if let Some(path) = value.strip_prefix("--file=") {
            return Some(PathBuf::from(path));
        }
    }

    let cwd = std::env::current_dir().ok()?;
    for dir in cwd.ancestors() {
        for candidate in config_candidates(dir) {
            if candidate.exists() {
                return Some(candidate);
            }
        }
    }

    dirs::home_dir().map(|home| home.join(".dev").join("config.toml"))
}

fn first_task_name(cfg: &config::DevConfig) -> Option<&str> {
    cfg.tasks
        .as_ref()
        .and_then(|tasks| tasks.keys().next().map(String::as_str))
}

fn summarize_languages(cfg: &config::DevConfig) -> Vec<String> {
    cfg.languages
        .as_ref()
        .map(|languages| {
            languages
                .iter()
                .map(|(name, language)| {
                    let verbs = language
                        .pipelines
                        .as_ref()
                        .map(pipeline_names)
                        .unwrap_or_default();
                    if verbs.is_empty() {
                        format!("{name} (no pipelines)")
                    } else {
                        format!("{name} ({})", verbs.join(","))
                    }
                })
                .collect()
        })
        .unwrap_or_default()
}

fn pipeline_names(pipelines: &config::Pipelines) -> Vec<&'static str> {
    let mut names = Vec::new();
    if pipelines.fmt.is_some() {
        names.push("fmt");
    }
    if pipelines.lint.is_some() {
        names.push("lint");
    }
    if pipelines.type_check.is_some() {
        names.push("type");
    }
    if pipelines.test.is_some() {
        names.push("test");
    }
    if pipelines.fix.is_some() {
        names.push("fix");
    }
    if pipelines.check.is_some() {
        names.push("check");
    }
    if pipelines.ci.is_some() {
        names.push("ci");
    }
    names
}

fn summarize_agents(cfg: &config::DevConfig) -> Vec<String> {
    cfg.agents
        .as_ref()
        .map(|agents| {
            let mut summaries = Vec::new();
            let default_agent = cfg.default_agent.as_deref();
            for (name, agent) in agents {
                let adapter = agent.adapter.as_deref().unwrap_or("codex");
                let model = agent.model.as_deref().unwrap_or("adapter-default");
                let default_marker = if Some(name.as_str()) == default_agent {
                    " default"
                } else {
                    ""
                };
                let iterations = agent
                    .iterations
                    .map(|value| format!(" x{value}"))
                    .unwrap_or_default();
                summaries.push(format!(
                    "{name} ({adapter}/{model}{iterations}{default_marker})"
                ));
            }
            summaries
        })
        .unwrap_or_default()
}

fn config_candidates(dir: &Path) -> [PathBuf; 2] {
    [
        dir.join(".dev").join("config.toml"),
        dir.join("tools").join("dev").join("config.toml"),
    ]
}
