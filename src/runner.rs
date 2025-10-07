use anyhow::Result;

use std::path::PathBuf;

use crate::cli::{
    BranchCreate, BranchFinalize, Cli, Command, ConfigCommand, EnvCommand, GitCommand, InstallArgs,
    LanguageCommand, ReleasePr, Verb, VersionCommand,
};
use crate::{gitops, scaffold, versioning};

#[derive(Clone, Debug)]
struct CliContext {
    chdir: Option<PathBuf>,
    file: Option<PathBuf>,
    language: Option<String>,
    dry_run: bool,
    verbose: u8,
    no_color: bool,
}

impl From<&Cli> for CliContext {
    fn from(cli: &Cli) -> Self {
        Self {
            chdir: cli.chdir.clone(),
            file: cli.file.clone(),
            language: cli.language.clone(),
            dry_run: cli.dry_run,
            verbose: cli.verbose,
            no_color: cli.no_color,
        }
    }
}

pub fn run(cli: Cli) -> Result<()> {
    let ctx = CliContext::from(&cli);
    match cli.command {
        Command::List => handle_list(&ctx),
        Command::Run { task } => handle_run(&ctx, &task),
        Command::Fmt => handle_verb(&ctx, Verb::Fmt),
        Command::Lint => handle_verb(&ctx, Verb::Lint),
        Command::TypeCheck => handle_verb(&ctx, Verb::TypeCheck),
        Command::Test => handle_verb(&ctx, Verb::Test),
        Command::Fix => handle_verb(&ctx, Verb::Fix),
        Command::Check => handle_verb(&ctx, Verb::Check),
        Command::Ci => handle_verb(&ctx, Verb::Ci),
        Command::All { verb } => handle_all(&ctx, verb),
        Command::Install(args) => handle_install(&ctx, args),
        Command::Language { command } => handle_language(&ctx, command),
        Command::Git { command } => handle_git(&ctx, command),
        Command::Version { command } => handle_version(&ctx, command),
        Command::Env { command } => handle_env(&ctx, command),
        Command::Config { command } => handle_config(&ctx, command),
    }
}

fn handle_list(_ctx: &CliContext) -> Result<()> {
    // TODO: enumerate tasks once indexing is implemented.
    Ok(())
}

fn handle_run(_ctx: &CliContext, task: &str) -> Result<()> {
    let _ = task;
    // TODO: resolve and execute named task or pipeline.
    Ok(())
}

fn handle_verb(_ctx: &CliContext, _verb: Verb) -> Result<()> {
    // TODO: map verb to pipeline for the selected language.
    Ok(())
}

fn handle_all(_ctx: &CliContext, _verb: Verb) -> Result<()> {
    // TODO: aggregate verb over all configured languages.
    Ok(())
}

fn handle_install(ctx: &CliContext, args: InstallArgs) -> Result<()> {
    let language = args.language.or_else(|| ctx.language.clone());
    let Some(language) = language else {
        // TODO: surface available languages from config.
        return Err(anyhow::anyhow!(
            "no language provided and no default configured; rerun with `dev install <language>`"
        ));
    };

    if ctx.dry_run {
        // TODO: integrate with structured logging once available.
        println!("[dry-run] would install tooling for {}", language);
        return Ok(());
    }

    scaffold::install(&language)
}

fn handle_language(_ctx: &CliContext, command: LanguageCommand) -> Result<()> {
    let _ = command;
    // TODO: update default language in config.
    Ok(())
}

fn handle_git(ctx: &CliContext, command: GitCommand) -> Result<()> {
    match command {
        GitCommand::BranchCreate(args) => handle_branch_create(ctx, args),
        GitCommand::BranchFinalize(args) => handle_branch_finalize(ctx, args),
        GitCommand::ReleasePr(args) => handle_release_pr(ctx, args),
    }
}

fn handle_branch_create(_ctx: &CliContext, args: BranchCreate) -> Result<()> {
    gitops::branch_create(&args)
}

fn handle_branch_finalize(_ctx: &CliContext, args: BranchFinalize) -> Result<()> {
    gitops::branch_finalize(&args)
}

fn handle_release_pr(_ctx: &CliContext, args: ReleasePr) -> Result<()> {
    gitops::release_pr(&args)
}

fn handle_version(_ctx: &CliContext, command: VersionCommand) -> Result<()> {
    versioning::handle(command)
}

fn handle_env(_ctx: &CliContext, command: Option<EnvCommand>) -> Result<()> {
    match command {
        Some(EnvCommand::List) | None => {
            // TODO: list .env entries.
            Ok(())
        }
        Some(EnvCommand::Add { key, value }) => {
            let _ = (key, value);
            // TODO: add or update .env entry.
            Ok(())
        }
        Some(EnvCommand::Rm { key }) => {
            let _ = key;
            // TODO: remove .env entry.
            Ok(())
        }
    }
}

fn handle_config(_ctx: &CliContext, command: Option<ConfigCommand>) -> Result<()> {
    match command {
        None | Some(ConfigCommand::Show) => {
            // TODO: display config path and summary.
            Ok(())
        }
        Some(ConfigCommand::Check) => {
            // TODO: validate config structure.
            Ok(())
        }
        Some(ConfigCommand::Generate { path }) => {
            let _ = path;
            // TODO: emit example config to path.
            Ok(())
        }
        Some(ConfigCommand::Reload) => {
            // TODO: refresh config cache.
            Ok(())
        }
    }
}
