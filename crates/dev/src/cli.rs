use std::path::PathBuf;

use anyhow::Result;
use clap::{Args, Parser, Subcommand, ValueEnum, error::ErrorKind};

use crate::cli_help::{dynamic_help, should_append_dynamic_help};

/// Top-level CLI definition matching the spec in `docs/spec.md`.
#[derive(Parser, Debug)]
#[command(
    name = "dev",
    version,
    about = "Unified developer workflows",
    long_about = "A single-binary developer workflow tool for configured tasks, language pipelines, git flows, setup, review reports, directory manifests, and environment management.",
    after_help = "Examples:\n  dev config generate\n  dev list\n  dev lint\n  dev run all_check\n  dev git branch-create feature/docs\n  dev setup status"
)]
pub struct Cli {
    /// Change working directory before loading config or running commands.
    #[arg(short = 'C', long = "chdir")]
    pub chdir: Option<PathBuf>,
    /// Use an explicit devkit config file.
    #[arg(short = 'f', long = "file")]
    pub file: Option<PathBuf>,
    /// Select a named project from config.
    #[arg(long = "project", global = true)]
    pub project: Option<String>,
    /// Override the configured default language.
    #[arg(short = 'l', long = "language")]
    pub language: Option<String>,
    /// Print planned commands without executing where supported.
    #[arg(short = 'n', long = "dry-run", global = true)]
    pub dry_run: bool,
    /// Increase logging verbosity; repeat for more detail.
    #[arg(short = 'v', long = "verbose", action = clap::ArgAction::Count)]
    pub verbose: u8,
    /// Disable colored output.
    #[arg(long = "no-color", global = true)]
    pub no_color: bool,
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// List available tasks and pipelines.
    #[command(after_help = "Examples:\n  dev list\n  dev --language rust list")]
    List,
    /// Execute a named task or pipeline.
    #[command(after_help = "Examples:\n  dev run rust_fmt\n  dev run all_check")]
    Run {
        /// Configured task or pipeline name to execute.
        task: String,
    },
    /// Start a long-running development server for the current project.
    #[command(after_help = "Examples:\n  dev start\n  dev start --port 5173\n  dev start --prod")]
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
    #[command(after_help = "Examples:\n  dev all check\n  dev all test")]
    All {
        /// Pipeline verb to run across all configured languages.
        verb: Verb,
    },
    /// Install tooling and scaffolds for a language (defaults to configured language).
    #[command(
        after_help = "Examples:\n  dev install\n  dev install rust --force\n  dev install typescript --no-scaffold"
    )]
    Install(InstallArgs),
    /// Manage language defaults.
    #[command(after_help = "Examples:\n  dev language set rust\n  dev language set python")]
    Language {
        #[command(subcommand)]
        command: LanguageCommand,
    },
    /// Git-centric flows such as branch management and release PRs.
    #[command(
        after_help = "Examples:\n  dev git branch-create feature/docs\n  dev git branch-create feature/docs --from main --push\n  dev git branch-finalize --delete\n  dev git release-pr patch --from main --to release-candidate"
    )]
    Git {
        #[command(subcommand)]
        command: GitCommand,
    },
    /// Version bumping, changelog, and tagging.
    #[command(
        after_help = "Examples:\n  dev version show\n  dev version bump patch --tag\n  dev version changelog --unreleased"
    )]
    Version {
        #[command(subcommand)]
        command: VersionCommand,
    },
    /// Check for and install newer dev releases.
    #[command(
        after_help = "Examples:\n  dev update --check\n  dev update --yes\n  dev update --version v0.4.0 --install-dir ~/.local/bin"
    )]
    Update(UpdateArgs),
    /// Environment variable helper commands backed by a `.env` file.
    #[command(
        after_help = "Examples:\n  dev env\n  dev env --raw\n  dev env get DATABASE_URL\n  dev env add API_URL http://localhost:3000\n  dev env switch staging\n  dev env check"
    )]
    Env(EnvArgs),
    /// Configuration display, validation, and template generation.
    #[command(
        after_help = "Examples:\n  dev config show\n  dev config path\n  dev config check\n  dev config generate\n  dev config add rust_fmt -- cargo fmt"
    )]
    Config {
        #[command(subcommand)]
        command: Option<ConfigCommand>,
    },
    /// System setup and installation management.
    #[command(
        after_help = "Examples:\n  dev setup\n  dev setup status\n  dev setup list\n  dev setup run rustup uv\n  dev setup all --skip-installed\n  dev setup inference comfyui --dest ~/repos/inference/dev-comfyui"
    )]
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
    #[command(
        after_help = "Examples:\n  dev review\n  dev review --main --output review.md\n  dev review --include-working"
    )]
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
    #[command(
        after_help = "Examples:\n  dev walk\n  dev walk crates/dev -o manifest.md --extensions .rs .toml\n  dev walk . --no-content --max-depth 4"
    )]
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
    #[command(after_help = "Examples:\n  dev language set rust\n  dev language set typescript")]
    Set {
        /// Language name to write as default_language.
        name: String,
    },
}

#[derive(Args, Debug)]
pub struct InstallArgs {
    /// Language to install/scaffold; defaults to --language or default_language.
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
    /// Create a branch from the resolved base branch.
    #[command(
        after_help = "Examples:\n  dev git branch-create feature/docs\n  dev git branch-create feature/docs --from main --push\n  dev git branch-create feature/docs --allow-dirty"
    )]
    BranchCreate(BranchCreate),
    /// Merge a feature branch into the resolved base branch.
    #[command(
        after_help = "Examples:\n  dev git branch-finalize\n  dev git branch-finalize feature/docs --into main --delete"
    )]
    BranchFinalize(BranchFinalize),
    /// Bump the release branch, update changelog/version files, and open a PR.
    #[command(
        after_help = "Examples:\n  dev git release-pr patch\n  dev git release-pr minor --from main --to release-candidate --no-open"
    )]
    ReleasePr(ReleasePr),
}

#[derive(Args, Debug)]
pub struct BranchCreate {
    /// Name of the branch to create.
    pub name: String,
    /// Base branch to create from; overrides configured/discovered base.
    #[arg(long = "from")]
    pub base: Option<String>,
    /// Push the new branch and set upstream tracking.
    #[arg(long)]
    pub push: bool,
    /// Allow branch creation with a dirty working tree.
    #[arg(long = "allow-dirty")]
    pub allow_dirty: bool,
}

#[derive(Args, Debug)]
pub struct BranchFinalize {
    /// Feature branch to merge; defaults to the current branch.
    #[arg()]
    pub name: Option<String>,
    /// Base branch to merge into; overrides configured/discovered base.
    #[arg(long = "into")]
    pub base: Option<String>,
    /// Delete the feature branch locally and remotely after merge.
    #[arg(long)]
    pub delete: bool,
    /// Allow finalization with a dirty working tree.
    #[arg(long = "allow-dirty")]
    pub allow_dirty: bool,
}

#[derive(Args, Debug)]
pub struct ReleasePr {
    /// Version bump level (major, minor, patch, prerelease)
    #[arg(value_enum)]
    pub bump: BumpLevel,
    /// PR base branch; defaults to configured/discovered main branch.
    #[arg(long = "from")]
    pub from: Option<String>,
    /// Release branch/head; defaults to [git].release_branch.
    #[arg(long = "to")]
    pub to: Option<String>,
    /// Prepare and push the release branch without opening a PR.
    #[arg(long = "no-open")]
    pub no_open: bool,
}

#[derive(Subcommand, Debug)]
pub enum VersionCommand {
    /// Update the package version and optionally commit/tag it.
    #[command(
        after_help = "Examples:\n  dev version bump patch\n  dev version bump minor --tag\n  dev version bump patch --custom 1.2.3 --no-commit"
    )]
    Bump(VersionBump),
    /// Show changelog entries for a git range.
    #[command(
        after_help = "Examples:\n  dev version changelog --unreleased\n  dev version changelog --since v1.2.0"
    )]
    Changelog(ChangelogArgs),
    /// Print the detected package version.
    #[command(after_help = "Examples:\n  dev version show")]
    Show,
}

#[derive(Args, Debug)]
pub struct VersionBump {
    /// Semantic version bump level.
    #[arg(value_enum)]
    pub level: BumpLevel,
    /// Explicit version to write instead of deriving one from the level.
    #[arg(long = "custom")]
    pub custom: Option<String>,
    /// Create a git tag for the new version.
    #[arg(long = "tag")]
    pub tag: bool,
    /// Update files but do not create a git commit.
    #[arg(long = "no-commit")]
    pub no_commit: bool,
    /// Skip changelog updates.
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
    /// Git ref/tag to start changelog output from.
    #[arg(long = "since")]
    pub since: Option<String>,
    /// Show unreleased changes.
    #[arg(long = "unreleased")]
    pub unreleased: bool,
}

#[derive(Args, Debug)]
pub struct UpdateArgs {
    /// Only check whether an update is available.
    #[arg(long = "check", default_value_t = false)]
    pub check: bool,
    /// Install this release tag instead of looking up the latest release.
    #[arg(long = "version")]
    pub version: Option<String>,
    /// GitHub repository to query, in owner/repo form.
    #[arg(long = "repo", default_value = "bakobiibizo/devkit")]
    pub repo: String,
    /// Directory where the dev binary should be installed.
    #[arg(long = "install-dir")]
    pub install_dir: Option<PathBuf>,
    /// Confirm installation without prompting.
    #[arg(long = "yes", short = 'y', default_value_t = false)]
    pub yes: bool,
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
    #[command(after_help = "Examples:\n  dev env\n  dev env list\n  dev env --raw list")]
    List,
    /// Get a single environment variable value
    #[command(after_help = "Examples:\n  dev env get DATABASE_URL")]
    Get { key: String },
    /// Add or update an environment variable
    #[command(after_help = "Examples:\n  dev env add DATABASE_URL postgres://localhost/dev")]
    Add { key: String, value: String },
    /// Remove an environment variable
    #[command(after_help = "Examples:\n  dev env rm DATABASE_URL")]
    Rm { key: String },
    /// List available environment profiles (.env.*)
    #[command(after_help = "Examples:\n  dev env profiles")]
    Profiles,
    /// Switch to a different environment profile
    #[command(after_help = "Examples:\n  dev env switch staging")]
    Switch { profile: String },
    /// Save current .env as a named profile
    #[command(after_help = "Examples:\n  dev env save staging")]
    Save { name: String },
    /// Validate .env against required keys in config
    #[command(after_help = "Examples:\n  dev env check")]
    Check,
    /// Initialize .env from .env.example if missing
    #[command(after_help = "Examples:\n  dev env init")]
    Init,
    /// Generate .env.example from current .env (values stripped)
    #[command(after_help = "Examples:\n  dev env template")]
    Template,
    /// Show diff between .env and a reference file
    #[command(after_help = "Examples:\n  dev env diff\n  dev env diff .env.production")]
    Diff {
        /// Reference file to compare against (default: .env.example)
        #[arg(default_value = ".env.example")]
        reference: String,
    },
    /// Interactively add missing keys from a reference file
    #[command(after_help = "Examples:\n  dev env sync\n  dev env sync .env.example")]
    Sync {
        /// Reference file to sync from (default: .env.example)
        #[arg(default_value = ".env.example")]
        reference: String,
    },
}

#[derive(Subcommand, Debug)]
pub enum ConfigCommand {
    /// Display the parsed active configuration.
    #[command(after_help = "Examples:\n  dev config\n  dev config show")]
    Show,
    /// Print the resolved configuration path.
    #[command(after_help = "Examples:\n  dev config path")]
    Path,
    /// Validate the active configuration.
    #[command(after_help = "Examples:\n  dev config check")]
    Check,
    /// Write the embedded example configuration.
    #[command(
        after_help = "Examples:\n  dev config generate\n  dev config generate .dev/config.toml --force"
    )]
    Generate {
        /// Destination path; defaults to ~/.dev/config.toml.
        #[arg()]
        path: Option<PathBuf>,
        /// Overwrite an existing file.
        #[arg(long = "force", default_value_t = false)]
        force: bool,
    },
    /// Reparse the active configuration and print its summary.
    #[command(after_help = "Examples:\n  dev config reload")]
    Reload,
    /// Add a task command to the active configuration.
    #[command(
        after_help = "Examples:\n  dev config add rust_fmt -- cargo fmt\n  dev config add ci --append -- cargo test --workspace"
    )]
    Add {
        /// Task name to create or update.
        #[arg()]
        name: Option<String>,
        /// Command argv to store for the task.
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        command: Vec<String>,
        /// Replace an existing task.
        #[arg(long = "force", default_value_t = false)]
        force: bool,
        /// Append the command to an existing task.
        #[arg(long = "append", default_value_t = false)]
        append: bool,
    },
}

#[derive(Subcommand, Debug)]
pub enum SetupCommand {
    /// Run default components with --skip-installed implied
    #[command(
        after_help = "Examples:\n  dev setup run rustup uv\n  dev setup run docker nvidia_container_runtime --skip-installed\n  dev setup run pm2 --no-deps"
    )]
    Run {
        /// Components to install
        components: Vec<String>,
        #[arg(long = "skip-installed")]
        skip_installed: bool,
        #[arg(long = "no-deps")]
        no_deps: bool,
    },
    /// Clone/update a dev-* inference repository and run its setup script.
    #[command(
        after_help = "Examples:\n  dev setup inference comfyui\n  dev setup inference llm --dest ~/repos/inference/dev-llm --no-cache"
    )]
    Inference {
        /// Inference service name; clones github.com/bakobiibizo/dev-<service>.
        #[arg()]
        service: String,
        /// Destination directory for the repository.
        #[arg(long = "dest")]
        dest: Option<PathBuf>,
        /// Remove a non-git destination before cloning.
        #[arg(long = "force", default_value_t = false)]
        force: bool,
        /// Pass --no-cache to the service setup script.
        #[arg(long = "no-cache", default_value_t = false)]
        no_cache: bool,
    },
    /// Run all compatible components
    #[command(
        after_help = "Examples:\n  dev setup all\n  dev setup all --skip-installed\n  dev setup all --no-deps"
    )]
    All {
        #[arg(long = "skip-installed")]
        skip_installed: bool,
        #[arg(long = "no-deps")]
        no_deps: bool,
    },
    /// Show installation status of all components
    #[command(after_help = "Examples:\n  dev setup status")]
    Status,
    /// List available components and their dependencies
    #[command(after_help = "Examples:\n  dev setup list")]
    List,
    /// Show effective setup configuration
    #[command(after_help = "Examples:\n  dev setup config")]
    Config,
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
