use std::path::PathBuf;
use std::process::Command as ProcessCommand;
use std::time::Instant;

use anyhow::{Context, Result, anyhow, bail};
use camino::Utf8PathBuf;

use crate::cli::{
    Cli, Command, ConfigCommand, EnvCommand, GitCommand, InstallArgs, LanguageCommand, Verb,
    VersionCommand,
};
use crate::config::DevConfig;
use crate::tasks::{CommandSpec, TaskIndex};
use crate::{config, gitops, scaffold, versioning};

pub fn run(cli: Cli) -> Result<()> {
    let ctx = CliContext::from(&cli);
    ctx.apply_chdir()?;

    match cli.command {
        Command::Config { command } => handle_config_only(&ctx, command),
        other => {
            let state = AppState::new(ctx)?;
            handle_with_state(&state, other)
        }
    }
}

fn handle_with_state(state: &AppState, command: Command) -> Result<()> {
    match command {
        Command::List => handle_list(state),
        Command::Run { task } => handle_run(state, &task),
        Command::Fmt => handle_verb(state, Verb::Fmt),
        Command::Lint => handle_verb(state, Verb::Lint),
        Command::TypeCheck => handle_verb(state, Verb::TypeCheck),
        Command::Test => handle_verb(state, Verb::Test),
        Command::Fix => handle_verb(state, Verb::Fix),
        Command::Check => handle_verb(state, Verb::Check),
        Command::Ci => handle_verb(state, Verb::Ci),
        Command::All { verb } => handle_all(state, verb),
        Command::Install(args) => handle_install(state, args),
        Command::Language { command } => handle_language(state, command),
        Command::Git { command } => handle_git(state, command),
        Command::Version { command } => handle_version(state, command),
        Command::Env { command } => handle_env(state, command),
        Command::Config { .. } => unreachable!("config commands handled earlier"),
    }
}

fn handle_list(state: &AppState) -> Result<()> {
    if state.tasks.is_empty() {
        println!("No tasks defined in {}.", state.config_path);
        return Ok(());
    }

    println!("Tasks defined in {}:", state.config_path);
    for name in state.tasks.task_names() {
        println!("  - {}", name);
    }
    Ok(())
}

fn handle_run(state: &AppState, task: &str) -> Result<()> {
    println!("Running task `{}`", task);
    let commands = state.tasks.flatten(task)?;
    execute_commands(state, task, &commands)
}

fn handle_verb(state: &AppState, verb: Verb) -> Result<()> {
    let language = state
        .effective_language(None)
        .ok_or_else(|| anyhow!("no language selected; pass --language or set default_language"))?;

    let tasks = pipeline_for_language(&state.config, &language, verb)
        .ok_or_else(|| anyhow!("language `{language}` has no `{}` pipeline", verb.as_str()))?;

    println!(
        "Running `{}` pipeline for language `{}`",
        verb.as_str(),
        language
    );
    run_task_sequence(state, &tasks)
}

fn handle_all(state: &AppState, verb: Verb) -> Result<()> {
    let languages = state
        .config
        .languages
        .as_ref()
        .ok_or_else(|| anyhow!("no languages configured"))?;

    let mut any_ran = false;
    for (language, spec) in languages {
        let Some(tasks) = spec
            .pipelines
            .as_ref()
            .and_then(|pipes| pipeline_lookup(pipes, verb).cloned())
        else {
            continue;
        };
        if !any_ran {
            println!("Running `{}` pipeline across languages:", verb.as_str());
        }
        any_ran = true;
        println!("- Language `{}`", language);
        run_task_sequence(state, &tasks)?;
    }

    if !any_ran {
        println!(
            "No languages define a `{}` pipeline; nothing to do.",
            verb.as_str()
        );
    }

    Ok(())
}

fn handle_install(state: &AppState, args: InstallArgs) -> Result<()> {
    let language = state.effective_language(args.language).ok_or_else(|| {
        anyhow!("no language selected; pass `dev install <language>` or configure default_language")
    })?;

    if state.ctx.dry_run {
        println!("[dry-run] would install tooling for `{}`", language);
        return Ok(());
    }

    println!("Installing tooling for `{}`...", language);
    scaffold::install(&language)
}

fn handle_language(_state: &AppState, command: LanguageCommand) -> Result<()> {
    match command {
        LanguageCommand::Set { name } => {
            println!("(todo) Update default language to `{name}` in config.");
            Ok(())
        }
    }
}

fn handle_git(_state: &AppState, command: GitCommand) -> Result<()> {
    match command {
        GitCommand::BranchCreate(args) => gitops::branch_create(&args),
        GitCommand::BranchFinalize(args) => gitops::branch_finalize(&args),
        GitCommand::ReleasePr(args) => gitops::release_pr(&args),
    }
}

fn handle_version(_state: &AppState, command: VersionCommand) -> Result<()> {
    versioning::handle(command)
}

fn handle_env(_state: &AppState, command: Option<EnvCommand>) -> Result<()> {
    match command {
        Some(EnvCommand::List) | None => {
            println!("(todo) Display environment variables");
            Ok(())
        }
        Some(EnvCommand::Add { key, value }) => {
            println!("(todo) Add `{key}` to .env with value `{value}`");
            Ok(())
        }
        Some(EnvCommand::Rm { key }) => {
            println!("(todo) Remove `{key}` from .env");
            Ok(())
        }
    }
}

fn handle_config_only(ctx: &CliContext, command: Option<ConfigCommand>) -> Result<()> {
    let config_path = ctx.resolve_config_path()?;
    match command {
        None | Some(ConfigCommand::Show) => {
            println!("{}", config_path);
            Ok(())
        }
        Some(ConfigCommand::Check) => {
            let config = config::load_from_path(&config_path)?;
            let _ = TaskIndex::from_config(&config)?;
            println!("Config OK: {}", config_path);
            Ok(())
        }
        Some(ConfigCommand::Generate { path }) => {
            let target = match path {
                Some(path) => Utf8PathBuf::from_path_buf(path)
                    .map_err(|_| anyhow!("config generate path must be valid UTF-8"))?,
                None => Utf8PathBuf::from(config_path.clone()),
            };
            println!("(todo) Generate config at {}", target);
            Ok(())
        }
        Some(ConfigCommand::Reload) => {
            println!("Config reloaded on each invocation of the CLI.");
            Ok(())
        }
    }
}

fn run_task_sequence(state: &AppState, tasks: &[String]) -> Result<()> {
    for task in tasks {
        handle_run(state, task)?;
    }
    Ok(())
}

fn execute_commands(state: &AppState, task: &str, commands: &[CommandSpec]) -> Result<()> {
    if commands.is_empty() {
        println!("Task `{}` has no commands.", task);
        return Ok(());
    }

    let total = commands.len();
    for (idx, spec) in commands.iter().enumerate() {
        let render = format_command(&spec.argv);
        println!("[{}/{}] {} :: {}", idx + 1, total, spec.origin, render);

        if state.ctx.dry_run {
            println!("    (dry-run) skipped");
            continue;
        }

        let start = Instant::now();
        let status = run_process(&spec.argv)?;
        if status.success() {
            println!("[ok] {} (completed in {:.2?})", render, start.elapsed());
        } else if spec.allow_fail {
            println!(
                "[warn] {} failed with exit code {:?} (ignored)",
                render,
                status.code()
            );
        } else {
            bail!(
                "command `{}` failed with exit code {:?}",
                render,
                status.code()
            );
        }
    }

    if state.ctx.dry_run {
        println!("Task `{}` simulated (dry-run).", task);
    } else {
        println!("Task `{}` completed successfully.", task);
    }

    Ok(())
}

fn run_process(argv: &[String]) -> Result<std::process::ExitStatus> {
    let mut command = ProcessCommand::new(&argv[0]);
    if argv.len() > 1 {
        command.args(&argv[1..]);
    }
    command
        .status()
        .with_context(|| format!("executing `{}`", format_command(argv)))
}

fn format_command(argv: &[String]) -> String {
    argv.iter()
        .map(|arg| {
            if arg.chars().any(|c| c.is_whitespace()) {
                let escaped = arg.replace('"', "\\\"");
                format!("\"{}\"", escaped)
            } else {
                arg.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn pipeline_for_language(config: &DevConfig, language: &str, verb: Verb) -> Option<Vec<String>> {
    let languages = config.languages.as_ref()?;
    let lang = languages.get(language)?;
    let pipelines = lang.pipelines.as_ref()?;
    pipeline_lookup(pipelines, verb).cloned()
}

fn pipeline_lookup<'a>(
    pipelines: &'a crate::config::Pipelines,
    verb: Verb,
) -> Option<&'a Vec<String>> {
    match verb {
        Verb::Fmt => pipelines.fmt.as_ref(),
        Verb::Lint => pipelines.lint.as_ref(),
        Verb::TypeCheck => pipelines.type_check.as_ref(),
        Verb::Test => pipelines.test.as_ref(),
        Verb::Fix => pipelines.fix.as_ref(),
        Verb::Check => pipelines.check.as_ref(),
        Verb::Ci => pipelines.ci.as_ref(),
    }
}

#[derive(Clone, Debug)]
struct CliContext {
    chdir: Option<PathBuf>,
    file: Option<PathBuf>,
    language: Option<String>,
    dry_run: bool,
    verbose: u8,
    no_color: bool,
}

impl CliContext {
    fn apply_chdir(&self) -> Result<()> {
        if let Some(path) = &self.chdir {
            std::env::set_current_dir(path)
                .with_context(|| format!("changing directory to {}", path.display()))?;
        }
        Ok(())
    }

    fn resolve_config_path(&self) -> Result<Utf8PathBuf> {
        if let Some(path) = &self.file {
            return Utf8PathBuf::from_path_buf(path.clone())
                .map_err(|_| anyhow!("config path must be valid UTF-8"));
        }

        let home = dirs::home_dir().ok_or_else(|| anyhow!("unable to determine home directory"))?;
        let mut path = home;
        path.push(".dev");
        path.push("config.toml");
        Utf8PathBuf::from_path_buf(path).map_err(|_| anyhow!("config path must be valid UTF-8"))
    }

    fn effective_language(
        &self,
        config: &DevConfig,
        override_lang: Option<String>,
    ) -> Option<String> {
        override_lang
            .or_else(|| self.language.clone())
            .or_else(|| config.default_language.clone())
    }
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

struct AppState {
    ctx: CliContext,
    config_path: Utf8PathBuf,
    config: DevConfig,
    tasks: TaskIndex,
}

impl AppState {
    fn new(ctx: CliContext) -> Result<Self> {
        let config_path = ctx.resolve_config_path()?;
        let config = config::load_from_path(&config_path)?;
        let tasks = TaskIndex::from_config(&config)?;
        Ok(Self {
            ctx,
            config_path,
            config,
            tasks,
        })
    }

    fn effective_language(&self, override_lang: Option<String>) -> Option<String> {
        self.ctx.effective_language(&self.config, override_lang)
    }
}
