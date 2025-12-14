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
    Env {
        #[command(subcommand)]
        command: Option<EnvCommand>,
    },
    /// Configuration display, validation, and template generation.
    Config {
        #[command(subcommand)]
        command: Option<ConfigCommand>,
    },
    #[command(external_subcommand)]
    External(Vec<String>),
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

#[derive(Subcommand, Debug)]
pub enum EnvCommand {
    List,
    Add { key: String, value: String },
    Rm { key: String },
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

/// Helper entry point so `main` can stay minimal.
pub fn parse() -> Cli {
    Cli::parse()
}
