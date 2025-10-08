use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Command as ProcessCommand, Stdio};
use std::thread;
use std::time::Instant;

use anyhow::{Context, Result, anyhow, bail};
use camino::Utf8PathBuf;

use crate::cli::{
    Cli, Command, ConfigCommand, EnvCommand, GitCommand, InstallArgs, LanguageCommand, Verb,
    VersionCommand,
};
use crate::config::DevConfig;
use crate::envfile;
use crate::tasks::{CommandSpec, TaskIndex};
use crate::{config, gitops, scaffold, versioning};

pub fn run(cli: Cli) -> Result<()> {
    let ctx = CliContext::from(&cli);
    ctx.apply_chdir()?;

    let _ = ctx.no_color;
    let _ = ctx.verbose;

    match cli.command {
        Command::Config { command } => handle_config_only(&ctx, command),
        Command::Language {
            command: LanguageCommand::Set { name },
        } => handle_language_set(&ctx, name),
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
        println!(
            "[dry-run] would install scaffolds and tooling for `{}`",
            language
        );
        return Ok(());
    }

    println!("Installing scaffolds for `{}`...", language);
    scaffold::install(&language)?;

    match install_commands(&state.config, &language) {
        Some(commands) if !commands.is_empty() => {
            println!("Running provisioning commands for `{}`:", language);
            for command in commands {
                run_external_command(&command)?;
            }
            Ok(())
        }
        _ => {
            println!("No provisioning commands configured for `{}`.", language);
            Ok(())
        }
    }
}

fn handle_language(state: &AppState, command: LanguageCommand) -> Result<()> {
    match command {
        LanguageCommand::Set { name } => handle_language_set(&state.ctx, name),
    }
}

fn handle_git(state: &AppState, command: GitCommand) -> Result<()> {
    match command {
        GitCommand::BranchCreate(args) => gitops::branch_create(&args, state.ctx.dry_run),
        GitCommand::BranchFinalize(args) => gitops::branch_finalize(&args, state.ctx.dry_run),
        GitCommand::ReleasePr(args) => gitops::release_pr(&args, state.ctx.dry_run),
    }
}

fn handle_version(state: &AppState, command: VersionCommand) -> Result<()> {
    versioning::handle(&state.config, state.ctx.dry_run, command)
}

fn handle_env(state: &AppState, command: Option<EnvCommand>) -> Result<()> {
    match command {
        Some(EnvCommand::List) | None => env_list(state),
        Some(EnvCommand::Add { key, value }) => env_add(state, &key, &value),
        Some(EnvCommand::Rm { key }) => env_remove(state, &key),
    }
}

fn env_list(state: &AppState) -> Result<()> {
    let env_path = state.env_path()?;
    let env = envfile::EnvFile::load(&env_path)?;
    let mut entries: Vec<_> = env.entries().collect();
    entries.sort_by(|a, b| a.0.cmp(b.0));

    if entries.is_empty() {
        println!("No environment variables defined in {}.", env.path());
        return Ok(());
    }

    println!("Environment variables in {}:", env.path());
    for (key, value) in entries {
        let mask = if value.is_empty() { "" } else { "*****" };
        println!("  {}={}", key, mask);
    }
    Ok(())
}

fn env_add(state: &AppState, key: &str, value: &str) -> Result<()> {
    let env_path = state.env_path()?;
    let mut env = envfile::EnvFile::load(&env_path)?;
    let existed = env.entries().any(|(existing, _)| existing == key);
    env.upsert(key, value);
    env.save()?;

    let target = env.path();
    if existed {
        println!("Updated {} in {}", key, target);
    } else {
        println!("Added {} to {}", key, target);
    }
    Ok(())
}

fn env_remove(state: &AppState, key: &str) -> Result<()> {
    let env_path = state.env_path()?;
    let mut env = envfile::EnvFile::load(&env_path)?;
    if env.remove(key) {
        env.save()?;
        println!("Removed {} from {}", key, env.path());
    } else {
        println!("Key {} not present in {}", key, env.path());
    }
    Ok(())
}

fn handle_config_only(ctx: &CliContext, command: Option<ConfigCommand>) -> Result<()> {
    let config_path = ctx.resolve_config_path()?;
    match command {
        None | Some(ConfigCommand::Show) => {
            if !config_path.exists() {
                println!("No config found at {}.", config_path);
                println!("Use `dev config generate` to scaffold a default configuration.");
                return Ok(());
            }

            let config = config::load_from_path(&config_path)?;
            println!("Config path: {}", config_path);
            println!("{}", config::format_summary(&config));
            Ok(())
        }
        Some(ConfigCommand::Check) => {
            let config = config::load_from_path(&config_path)?;
            let _ = TaskIndex::from_config(&config)?;
            println!("Config OK: {}", config_path);
            println!("{}", config::format_summary(&config));
            Ok(())
        }
        Some(ConfigCommand::Generate { path, force }) => {
            let target = match path {
                Some(path) => Utf8PathBuf::from_path_buf(path)
                    .map_err(|_| anyhow!("config generate path must be valid UTF-8"))?,
                None => config_path.clone(),
            };
            config::write_example_config(&target, force)?;
            if force {
                println!("Overwrote config at {}", target);
            } else {
                println!("Wrote example config to {}", target);
            }
            Ok(())
        }
        Some(ConfigCommand::Reload) => {
            if !config_path.exists() {
                println!("No config found at {}. Nothing to reload.", config_path);
                return Ok(());
            }
            let config = config::load_from_path(&config_path)?;
            println!("Reloaded config from {}", config_path);
            println!("{}", config::format_summary(&config));
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

fn run_external_command(argv: &[String]) -> Result<()> {
    if argv.is_empty() {
        bail!("invalid installer command: empty argv");
    }
    println!("  -> {}", format_command(argv));
    let status = run_process_streaming(argv)?;

    if status.success() {
        println!("     [ok]");
        Ok(())
    } else {
        bail!(
            "installer command `{}` failed with exit code {:?}",
            format_command(argv),
            status.code()
        )
    }
}

fn run_process_streaming(argv: &[String]) -> Result<std::process::ExitStatus> {
    let mut command = ProcessCommand::new(&argv[0]);
    if argv.len() > 1 {
        command.args(&argv[1..]);
    }
    command.stdout(Stdio::piped()).stderr(Stdio::piped());

    let mut child = command
        .spawn()
        .with_context(|| format!("executing `{}`", format_command(argv)))?;

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    let stdout_handle = stdout.map(|pipe| {
        thread::spawn(move || {
            for line in BufReader::new(pipe).lines().map_while(Result::ok) {
                println!("     stdout | {}", line);
            }
        })
    });

    let stderr_handle = stderr.map(|pipe| {
        thread::spawn(move || {
            for line in BufReader::new(pipe).lines().map_while(Result::ok) {
                println!("     stderr | {}", line);
            }
        })
    });

    if let Some(handle) = stdout_handle {
        let _ = handle.join();
    }
    if let Some(handle) = stderr_handle {
        let _ = handle.join();
    }

    child
        .wait()
        .with_context(|| format!("waiting on `{}`", format_command(argv)))
}

fn pipeline_for_language(config: &DevConfig, language: &str, verb: Verb) -> Option<Vec<String>> {
    let languages = config.languages.as_ref()?;
    let lang = languages.get(language)?;
    let pipelines = lang.pipelines.as_ref()?;
    pipeline_lookup(pipelines, verb).cloned()
}

fn pipeline_lookup(pipelines: &crate::config::Pipelines, verb: Verb) -> Option<&Vec<String>> {
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

fn install_commands(config: &DevConfig, language: &str) -> Option<Vec<Vec<String>>> {
    let languages = config.languages.as_ref()?;
    let lang = languages.get(language)?;
    lang.install.clone()
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

    fn env_path(&self) -> Result<Utf8PathBuf> {
        let cwd = envfile::current_working_dir()?;
        envfile::locate(&cwd)
    }
}
fn handle_language_set(ctx: &CliContext, name: String) -> Result<()> {
    let path = ctx.resolve_config_path()?;
    config::set_default_language(&path, &name)?;
    println!("Default language set to `{}` in {}", name, path);
    println!("Reload config to apply for this session.");
    Ok(())
}
