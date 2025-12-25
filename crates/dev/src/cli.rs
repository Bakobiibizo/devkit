use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};

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
    Run {
        task: String,
    },
    /// Start a long-running development server for the current project.
    Start(StartArgs),
    /// Standard verbs dispatch to the current or selected language pipeline.
    Fmt,
    Lint,
    #[command(name = "type")]
    TypeCheck,
    Test,
    Fix,
    Check,
    Ci,
    /// Run aggregations across all languages for a given verb.
    All {
        verb: Verb,
    },
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
    /// Docker helpers for generating base/project containers.
    Docker {
        #[command(subcommand)]
        command: DockerCommand,
    },
    #[command(external_subcommand)]
    External(Vec<String>),
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
    #[arg(long = "base-image", default_value = "nvcr.io/nvidia/pytorch:25.09-py3")]
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

/// Helper entry point so `main` can stay minimal.
pub fn parse() -> Cli {
    Cli::parse()
}
