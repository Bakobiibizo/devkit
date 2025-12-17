use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command as ProcessCommand, Stdio};
use std::thread;
use std::time::Instant;
use std::{fs, io};

use anyhow::{Context, Result, anyhow, bail};
use camino::Utf8PathBuf;
use clap::Parser;

use crate::cli::{
    Cli, Command, ConfigCommand, EnvArgs, EnvCommand, GitCommand, InstallArgs, LanguageCommand,
    StartArgs, Verb, VersionCommand,
};
use crate::config::{DevConfig, TaskUpdateMode};
use crate::envfile;
use crate::tasks::{CommandSpec, TaskIndex};
use crate::{config, gitops, scaffold, versioning};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ConfigPathSource {
    Explicit,
    Discovered,
    HomeDefault,
}

fn should_scaffold_in_cwd(language: &str) -> bool {
    // `dev install` is used both for bootstrapping a brand new project and for provisioning
    // dependencies in an existing one. When we detect common manifests in the current
    // directory, we skip template scaffolding to avoid overwriting/adding unrelated files.
    match language {
        "typescript" | "ts" => !Path::new("package.json").exists(),
        "python" => !Path::new("pyproject.toml").exists(),
        "rust" => !Path::new("Cargo.toml").exists(),
        _ => true,
    }
}

impl ConfigPathSource {
    fn as_str(&self) -> &'static str {
        match self {
            ConfigPathSource::Explicit => "explicit",
            ConfigPathSource::Discovered => "discovered",
            ConfigPathSource::HomeDefault => "home-default",
        }
    }
}

#[derive(Clone, Debug)]
struct ResolvedConfigPath {
    path: Utf8PathBuf,
    source: ConfigPathSource,
}

pub fn run(cli: Cli) -> Result<()> {
    let cli = normalize_external(cli)?;
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
        Command::Start(args) => handle_start(state, args),
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
        Command::Env(args) => handle_env(state, args),
        Command::Config { .. } => unreachable!("config commands handled earlier"),
        Command::External(extra) => {
            bail!("unknown command: {}", extra.join(" "))
        }
    }
}

fn normalize_external(cli: Cli) -> Result<Cli> {
    let Command::External(extra) = &cli.command else {
        return Ok(cli);
    };

    if extra.is_empty() {
        return Ok(cli);
    }

    let mut argv: Vec<String> = Vec::new();
    argv.push("dev".to_owned());

    if let Some(chdir) = &cli.chdir {
        argv.push("--chdir".to_owned());
        argv.push(chdir.to_string_lossy().to_string());
    }

    if let Some(file) = &cli.file {
        argv.push("--file".to_owned());
        argv.push(file.to_string_lossy().to_string());
    }

    if let Some(language) = &cli.language {
        argv.push("--language".to_owned());
        argv.push(language.clone());
    }

    if cli.dry_run {
        argv.push("--dry-run".to_owned());
    }

    if cli.no_color {
        argv.push("--no-color".to_owned());
    }

    for _ in 0..cli.verbose {
        argv.push("--verbose".to_owned());
    }

    argv.push("--project".to_owned());
    argv.push(extra[0].clone());

    argv.extend(extra[1..].iter().cloned());

    Cli::try_parse_from(argv).map_err(|err| anyhow!(err.to_string()))
}

fn handle_list(state: &AppState) -> Result<()> {
    if state.tasks.is_empty() {
        println!(
            "No tasks defined in {} ({}).",
            state.config_path,
            state.config_source.as_str()
        );
        return Ok(());
    }

    println!(
        "Tasks defined in {} ({}):",
        state.config_path,
        state.config_source.as_str()
    );
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

fn handle_start(state: &AppState, args: StartArgs) -> Result<()> {
    let mut argv = vec![
        "pnpm".to_owned(),
        "run".to_owned(),
        "dev".to_owned(),
        "--host".to_owned(),
    ];

    let port = args.port.or_else(|| if args.prod { Some(8091) } else { None });
    if let Some(port) = port {
        argv.push("--port".to_owned());
        argv.push(port.to_string());
    }

    println!("Starting dev server: {}", format_command(&argv));
    if state.ctx.dry_run {
        println!("    (dry-run) skipped");
        return Ok(());
    }

    let status = run_process(&argv)?;
    if status.success() {
        Ok(())
    } else {
        bail!(
            "command `{}` failed with exit code {:?}",
            format_command(&argv),
            status.code()
        )
    }
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

    if should_scaffold_in_cwd(&language) {
        println!("Installing scaffolds for `{}`...", language);
        scaffold::install(&language)?;
    } else {
        println!("Skipping scaffolds for `{}` (project already initialized)", language);
    }

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
        GitCommand::ReleasePr(args) => gitops::release_pr(&args, state.ctx.dry_run, &state.config),
    }
}

fn handle_version(state: &AppState, command: VersionCommand) -> Result<()> {
    versioning::handle(&state.config, state.ctx.dry_run, command)
}

fn handle_env(state: &AppState, args: EnvArgs) -> Result<()> {
    match args.command {
        Some(EnvCommand::List) | None => env_list(state, args.raw),
        Some(EnvCommand::Get { key }) => env_get(state, &key),
        Some(EnvCommand::Add { key, value }) => env_add(state, &key, &value),
        Some(EnvCommand::Rm { key }) => env_remove(state, &key),
        Some(EnvCommand::Profiles) => env_profiles(state),
        Some(EnvCommand::Switch { profile }) => env_switch(state, &profile),
        Some(EnvCommand::Save { name }) => env_save(state, &name),
        Some(EnvCommand::Check) => env_check(state),
        Some(EnvCommand::Init) => env_init(state),
        Some(EnvCommand::Template) => env_template(state),
        Some(EnvCommand::Diff { reference }) => env_diff(state, &reference),
        Some(EnvCommand::Sync { reference }) => env_sync(state, &reference),
    }
}

fn env_list(state: &AppState, raw: bool) -> Result<()> {
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
        if raw {
            println!("  {}={}", key, value);
        } else {
            let mask = if value.is_empty() { "" } else { "*****" };
            println!("  {}={}", key, mask);
        }
    }
    Ok(())
}

fn env_get(state: &AppState, key: &str) -> Result<()> {
    let env_path = state.env_path()?;
    let env = envfile::EnvFile::load(&env_path)?;

    for (k, v) in env.entries() {
        if k == key {
            println!("{}", v);
            return Ok(());
        }
    }

    bail!("key `{}` not found in {}", key, env.path())
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

fn env_profiles(state: &AppState) -> Result<()> {
    let env_path = state.env_path()?;
    let dir = env_path
        .parent()
        .ok_or_else(|| anyhow!("cannot determine parent directory of {}", env_path))?;

    let mut profiles: Vec<String> = Vec::new();
    for entry in fs::read_dir(dir.as_std_path())? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with(".env.") && !name.ends_with(".example") {
            let profile = name.strip_prefix(".env.").unwrap_or(&name);
            profiles.push(profile.to_owned());
        }
    }

    if profiles.is_empty() {
        println!("No environment profiles found in {}.", dir);
        println!("Use `dev env save <name>` to create a profile.");
        return Ok(());
    }

    profiles.sort();
    println!("Available profiles in {}:", dir);
    for profile in profiles {
        println!("  - {}", profile);
    }
    Ok(())
}

fn env_switch(state: &AppState, profile: &str) -> Result<()> {
    let env_path = state.env_path()?;
    let dir = env_path
        .parent()
        .ok_or_else(|| anyhow!("cannot determine parent directory of {}", env_path))?;

    let profile_path = dir.join(format!(".env.{}", profile));
    if !profile_path.exists() {
        bail!(
            "profile `{}` not found at {}. Use `dev env profiles` to list available profiles.",
            profile,
            profile_path
        );
    }

    fs::copy(profile_path.as_std_path(), env_path.as_std_path())
        .with_context(|| format!("copying {} to {}", profile_path, env_path))?;

    println!("Switched to profile `{}` (copied {} to {})", profile, profile_path, env_path);
    Ok(())
}

fn env_save(state: &AppState, name: &str) -> Result<()> {
    let env_path = state.env_path()?;
    if !env_path.exists() {
        bail!("no .env file found at {}", env_path);
    }

    let dir = env_path
        .parent()
        .ok_or_else(|| anyhow!("cannot determine parent directory of {}", env_path))?;

    let profile_path = dir.join(format!(".env.{}", name));
    fs::copy(env_path.as_std_path(), profile_path.as_std_path())
        .with_context(|| format!("copying {} to {}", env_path, profile_path))?;

    println!("Saved current .env as profile `{}` at {}", name, profile_path);
    Ok(())
}

fn env_check(state: &AppState) -> Result<()> {
    let env_path = state.env_path()?;
    let env = envfile::EnvFile::load(&env_path)?;
    let entries: std::collections::HashSet<_> = env.entries().map(|(k, _)| k.to_owned()).collect();

    let required = state.config.env.as_ref().and_then(|e| e.required.as_ref());
    let optional = state.config.env.as_ref().and_then(|e| e.optional.as_ref());

    let mut missing_required: Vec<&str> = Vec::new();
    let mut empty_required: Vec<&str> = Vec::new();
    let mut missing_optional: Vec<&str> = Vec::new();

    if let Some(required) = required {
        for key in required {
            if !entries.contains(key.as_str()) {
                missing_required.push(key);
            } else {
                let value = env.entries().find(|(k, _)| k == key).map(|(_, v)| v).unwrap_or("");
                if value.is_empty() {
                    empty_required.push(key);
                }
            }
        }
    }

    if let Some(optional) = optional {
        for key in optional {
            if !entries.contains(key.as_str()) {
                missing_optional.push(key);
            }
        }
    }

    println!("Checking {} against config requirements...", env_path);

    if missing_required.is_empty() && empty_required.is_empty() {
        println!("[ok] All required keys present and non-empty.");
    } else {
        if !missing_required.is_empty() {
            println!("[error] Missing required keys:");
            for key in &missing_required {
                println!("  - {}", key);
            }
        }
        if !empty_required.is_empty() {
            println!("[error] Empty required keys:");
            for key in &empty_required {
                println!("  - {}", key);
            }
        }
    }

    if !missing_optional.is_empty() {
        println!("[warn] Missing optional keys:");
        for key in &missing_optional {
            println!("  - {}", key);
        }
    }

    if !missing_required.is_empty() || !empty_required.is_empty() {
        bail!("environment validation failed");
    }

    Ok(())
}

fn env_init(state: &AppState) -> Result<()> {
    let env_path = state.env_path()?;
    if env_path.exists() {
        println!(".env already exists at {}. Nothing to do.", env_path);
        return Ok(());
    }

    let dir = env_path
        .parent()
        .ok_or_else(|| anyhow!("cannot determine parent directory of {}", env_path))?;

    let example_path = dir.join(".env.example");
    if !example_path.exists() {
        bail!(
            "no .env.example found at {}. Create one first or use `dev env template` to generate it.",
            example_path
        );
    }

    fs::copy(example_path.as_std_path(), env_path.as_std_path())
        .with_context(|| format!("copying {} to {}", example_path, env_path))?;

    println!("Initialized .env from {} at {}", example_path, env_path);
    Ok(())
}

fn env_template(state: &AppState) -> Result<()> {
    let env_path = state.env_path()?;
    let env = envfile::EnvFile::load(&env_path)?;

    let dir = env_path
        .parent()
        .ok_or_else(|| anyhow!("cannot determine parent directory of {}", env_path))?;

    let example_path = dir.join(".env.example");

    let mut output = String::new();
    output.push_str("# Environment template generated from .env\n");
    output.push_str("# Fill in the values for your environment\n\n");

    for (key, _) in env.entries() {
        output.push_str(&format!("{}=\n", key));
    }

    fs::write(example_path.as_std_path(), &output)
        .with_context(|| format!("writing {}", example_path))?;

    println!("Generated .env.example at {}", example_path);
    Ok(())
}

fn env_diff(state: &AppState, reference: &str) -> Result<()> {
    let env_path = state.env_path()?;
    let env = envfile::EnvFile::load(&env_path)?;
    let env_keys: std::collections::HashSet<_> = env.entries().map(|(k, _)| k.to_owned()).collect();

    let dir = env_path
        .parent()
        .ok_or_else(|| anyhow!("cannot determine parent directory of {}", env_path))?;

    let ref_path = dir.join(reference);
    if !ref_path.exists() {
        bail!("reference file not found at {}", ref_path);
    }

    let ref_env = envfile::EnvFile::load(&ref_path)?;
    let ref_keys: std::collections::HashSet<_> = ref_env.entries().map(|(k, _)| k.to_owned()).collect();

    let missing: Vec<_> = ref_keys.difference(&env_keys).collect();
    let extra: Vec<_> = env_keys.difference(&ref_keys).collect();

    println!("Comparing {} against {}:", env_path, ref_path);

    if missing.is_empty() && extra.is_empty() {
        println!("[ok] No differences found.");
        return Ok(());
    }

    if !missing.is_empty() {
        println!("Missing in .env (present in {}):", reference);
        for key in &missing {
            println!("  - {}", key);
        }
    }

    if !extra.is_empty() {
        println!("Extra in .env (not in {}):", reference);
        for key in &extra {
            println!("  + {}", key);
        }
    }

    Ok(())
}

fn env_sync(state: &AppState, reference: &str) -> Result<()> {
    let env_path = state.env_path()?;
    let mut env = envfile::EnvFile::load(&env_path)?;
    let env_keys: std::collections::HashSet<_> = env.entries().map(|(k, _)| k.to_owned()).collect();

    let dir = env_path
        .parent()
        .ok_or_else(|| anyhow!("cannot determine parent directory of {}", env_path))?;

    let ref_path = dir.join(reference);
    if !ref_path.exists() {
        bail!("reference file not found at {}", ref_path);
    }

    let ref_env = envfile::EnvFile::load(&ref_path)?;
    let ref_keys: std::collections::HashSet<_> = ref_env.entries().map(|(k, _)| k.to_owned()).collect();

    let missing: Vec<_> = ref_keys.difference(&env_keys).cloned().collect();

    if missing.is_empty() {
        println!("No missing keys. {} is in sync with {}.", env_path, ref_path);
        return Ok(());
    }

    println!("Adding {} missing keys from {}:", missing.len(), reference);
    for key in &missing {
        let value = ref_env.entries().find(|(k, _)| k == key).map(|(_, v)| v).unwrap_or("");
        env.upsert(key, value);
        println!("  + {}={}", key, if value.is_empty() { "(empty)" } else { "*****" });
    }

    env.save()?;
    println!("Synced {} keys to {}", missing.len(), env_path);
    Ok(())
}

fn handle_config_only(ctx: &CliContext, command: Option<ConfigCommand>) -> Result<()> {
    let resolved = ctx.resolve_config_path()?;
    let config_path = resolved.path;
    match command {
        Some(ConfigCommand::Path) => {
            println!("Config path: {} ({})", config_path, resolved.source.as_str());
            Ok(())
        }
        None | Some(ConfigCommand::Show) => {
            if !config_path.exists() {
                println!("No config found at {}.", config_path);
                println!("Use `dev config generate` to scaffold a default configuration.");
                return Ok(());
            }

            let config = config::load_from_path(&config_path)?;
            println!("Config path: {} ({})", config_path, resolved.source.as_str());
            println!("{}", config::format_summary(&config));
            Ok(())
        }
        Some(ConfigCommand::Check) => {
            let config = config::load_from_path(&config_path)?;
            let _ = TaskIndex::from_config(&config)?;
            println!("Config OK: {} ({})", config_path, resolved.source.as_str());
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
            println!("Reloaded config from {} ({})", config_path, resolved.source.as_str());
            println!("{}", config::format_summary(&config));
            Ok(())
        }
        Some(ConfigCommand::Add {
            name,
            command,
            force,
            append,
        }) => config_add(&config_path, name, command, force, append),
    }
}

fn config_add(
    config_path: &Utf8PathBuf,
    name: Option<String>,
    command: Vec<String>,
    force: bool,
    append: bool,
) -> Result<()> {
    let mut name = name;
    let mut command = command;

    if name.is_none() {
        name = Some(prompt("Task name: ")?);
    }
    let name = name
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow!("task name is required"))?;

    if command.is_empty() {
        let cmd = prompt("Command: ")?;
        if cmd.trim().is_empty() {
            bail!("command is required");
        }
        command = vec![cmd];
    }

    let (argv, render) = parse_config_add_command(&command)?;

    let existed = task_exists(config_path, &name)?;
    let mode = if append {
        TaskUpdateMode::Append
    } else if force {
        TaskUpdateMode::Overwrite
    } else if existed {
        let choice = prompt("Task exists. Overwrite, append, or cancel? (o/a/N): ")?;
        match choice.trim().to_lowercase().as_str() {
            "o" | "overwrite" => TaskUpdateMode::Overwrite,
            "a" | "append" => TaskUpdateMode::Append,
            _ => bail!("canceled"),
        }
    } else {
        TaskUpdateMode::Overwrite
    };

    println!("Config path: {}", config_path);
    println!("Task: {}", name);
    println!("Command: {}", render);
    if force || append || !existed {
        config::upsert_task_command(config_path, &name, &argv, mode)?;
        println!("Wrote task `{}` to {}", name, config_path);
        return Ok(());
    }

    let confirm = prompt("Write changes? (y/N): ")?;
    if confirm.trim().eq_ignore_ascii_case("y") {
        config::upsert_task_command(config_path, &name, &argv, mode)?;
        println!("Wrote task `{}` to {}", name, config_path);
        Ok(())
    } else {
        bail!("canceled")
    }
}

fn parse_config_add_command(command: &[String]) -> Result<(Vec<String>, String)> {
    if command.is_empty() {
        bail!("command is required");
    }

    if command.len() >= 2 && command[0] == "--" {
        let argv = command[1..].to_vec();
        if argv.is_empty() {
            bail!("argv after `--` must not be empty");
        }
        let render = format_command(&argv);
        return Ok((argv, render));
    }

    let cmd = command.join(" ");
    let argv = vec!["bash".to_owned(), "-lc".to_owned(), cmd.clone()];
    Ok((argv, format!("bash -lc {}", cmd)))
}

fn prompt(label: &str) -> Result<String> {
    print!("{}", label);
    io::Write::flush(&mut io::stdout()).with_context(|| format!("writing prompt `{label}`"))?;
    let mut buf = String::new();
    io::stdin()
        .read_line(&mut buf)
        .with_context(|| format!("reading input for `{label}`"))?;
    Ok(buf.trim_end_matches(['\n', '\r']).to_owned())
}

fn task_exists(path: &Utf8PathBuf, task_name: &str) -> Result<bool> {
    if !path.exists() {
        return Ok(false);
    }

    let raw = fs::read_to_string(path).with_context(|| format!("reading config {}", path))?;
    let doc: toml_edit::DocumentMut = raw
        .parse()
        .with_context(|| format!("parsing config {}", path))?;
    let Some(tasks) = doc.get("tasks").and_then(|item| item.as_table()) else {
        return Ok(false);
    };
    Ok(tasks.contains_key(task_name))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_temp_dir() -> Utf8PathBuf {
        let mut dir = std::env::temp_dir();
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        dir.push(format!("devkit-test-{ts}"));
        Utf8PathBuf::from_path_buf(dir).unwrap()
    }

    #[test]
    fn resolve_config_prefers_nearest_discovered() {
        let root = unique_temp_dir();
        let nested = root.join("a").join("b");
        fs::create_dir_all(nested.as_std_path()).unwrap();
        fs::create_dir_all(root.join(".dev").as_std_path()).unwrap();
        let cfg = root.join(".dev").join("config.toml");
        fs::write(cfg.as_std_path(), "default_language = 'python'\n").unwrap();

        let old = std::env::current_dir().unwrap();
        std::env::set_current_dir(nested.as_std_path()).unwrap();

        let ctx = CliContext {
            chdir: None,
            file: None,
            project: None,
            language: None,
            dry_run: false,
            verbose: 0,
            no_color: false,
        };
        let resolved = ctx.resolve_config_path().unwrap();
        assert_eq!(resolved.source, ConfigPathSource::Discovered);
        assert!(resolved.path.ends_with(".dev/config.toml"));

        std::env::set_current_dir(old).unwrap();
        let _ = fs::remove_dir_all(root.as_std_path());
    }

    #[test]
    fn resolve_config_prefers_legacy_when_no_dotdev() {
        let root = unique_temp_dir();
        let nested = root.join("a").join("b");
        fs::create_dir_all(nested.as_std_path()).unwrap();
        fs::create_dir_all(root.join("tools").join("dev").as_std_path()).unwrap();
        let cfg = root.join("tools").join("dev").join("config.toml");
        fs::write(cfg.as_std_path(), "default_language = 'python'\n").unwrap();

        let old = std::env::current_dir().unwrap();
        std::env::set_current_dir(nested.as_std_path()).unwrap();

        let ctx = CliContext {
            chdir: None,
            file: None,
            project: None,
            language: None,
            dry_run: false,
            verbose: 0,
            no_color: false,
        };
        let resolved = ctx.resolve_config_path().unwrap();
        assert_eq!(resolved.source, ConfigPathSource::Discovered);
        assert!(resolved.path.ends_with("tools/dev/config.toml"));

        std::env::set_current_dir(old).unwrap();
        let _ = fs::remove_dir_all(root.as_std_path());
    }

    #[test]
    fn resolve_config_prefers_explicit_file() {
        let root = unique_temp_dir();
        fs::create_dir_all(root.as_std_path()).unwrap();
        let cfg = root.join("explicit.toml");
        fs::write(cfg.as_std_path(), "default_language = 'python'\n").unwrap();

        let ctx = CliContext {
            chdir: None,
            file: Some(cfg.as_std_path().to_path_buf()),
            project: None,
            language: None,
            dry_run: false,
            verbose: 0,
            no_color: false,
        };
        let resolved = ctx.resolve_config_path().unwrap();
        assert_eq!(resolved.source, ConfigPathSource::Explicit);
        assert!(resolved.path.ends_with("explicit.toml"));

        let _ = fs::remove_dir_all(root.as_std_path());
    }

    #[test]
    fn project_applies_chdir_and_language() {
        let root = unique_temp_dir();
        let proj_dir = root.join("web");
        fs::create_dir_all(proj_dir.as_std_path()).unwrap();
        fs::create_dir_all(root.join(".dev").as_std_path()).unwrap();
        let cfg = root.join(".dev").join("config.toml");
        fs::write(
            cfg.as_std_path(),
            r#"default_language = 'python'

[projects.web]
chdir = 'web'
language = 'typescript'
"#,
        )
        .unwrap();

        let old = std::env::current_dir().unwrap();
        std::env::set_current_dir(root.as_std_path()).unwrap();

        let ctx = CliContext {
            chdir: None,
            file: None,
            project: Some("web".to_owned()),
            language: None,
            dry_run: false,
            verbose: 0,
            no_color: false,
        };
        let state = AppState::new(ctx).unwrap();
        assert_eq!(
            std::env::current_dir().unwrap(),
            proj_dir.as_std_path().to_path_buf()
        );
        assert_eq!(state.effective_language(None).as_deref(), Some("typescript"));

        std::env::set_current_dir(old).unwrap();
        let _ = fs::remove_dir_all(root.as_std_path());
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
    project: Option<String>,
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

    fn resolve_config_path(&self) -> Result<ResolvedConfigPath> {
        if let Some(path) = &self.file {
            let path = Utf8PathBuf::from_path_buf(path.clone())
                .map_err(|_| anyhow!("config path must be valid UTF-8"))?;
            return Ok(ResolvedConfigPath {
                path,
                source: ConfigPathSource::Explicit,
            });
        }

        if let Ok(cwd) = std::env::current_dir() {
            if let Ok(mut dir) = Utf8PathBuf::from_path_buf(cwd) {
                loop {
                    let preferred = dir.join(".dev").join("config.toml");
                    if preferred.exists() {
                        return Ok(ResolvedConfigPath {
                            path: preferred,
                            source: ConfigPathSource::Discovered,
                        });
                    }

                    let legacy = dir.join("tools").join("dev").join("config.toml");
                    if legacy.exists() {
                        return Ok(ResolvedConfigPath {
                            path: legacy,
                            source: ConfigPathSource::Discovered,
                        });
                    }

                    let Some(parent) = dir.parent() else {
                        break;
                    };
                    dir = parent.to_path_buf();
                }
            }
        }

        let home = dirs::home_dir().ok_or_else(|| anyhow!("unable to determine home directory"))?;
        let mut path = home;
        path.push(".dev");
        path.push("config.toml");
        let path = Utf8PathBuf::from_path_buf(path).map_err(|_| anyhow!("config path must be valid UTF-8"))?;
        Ok(ResolvedConfigPath {
            path,
            source: ConfigPathSource::HomeDefault,
        })
    }

    fn effective_language(
        &self,
        config: &DevConfig,
        project_language: Option<&str>,
        override_lang: Option<String>,
    ) -> Option<String> {
        override_lang
            .or_else(|| self.language.clone())
            .or_else(|| project_language.map(|s| s.to_owned()))
            .or_else(|| config.default_language.clone())
    }
}

impl From<&Cli> for CliContext {
    fn from(cli: &Cli) -> Self {
        Self {
            chdir: cli.chdir.clone(),
            file: cli.file.clone(),
            project: cli.project.clone(),
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
    config_source: ConfigPathSource,
    config: DevConfig,
    project_language: Option<String>,
    tasks: TaskIndex,
}

impl AppState {
    fn new(ctx: CliContext) -> Result<Self> {
        let resolved = ctx.resolve_config_path()?;
        let config_path = resolved.path;
        let config_source = resolved.source;
        let config = config::load_from_path(&config_path)?;

        let requested_project = ctx
            .project
            .clone()
            .or_else(|| config.default_project.clone());
        let mut project_language: Option<String> = None;

        if let Some(project) = requested_project.as_deref() {
            let projects = config
                .projects
                .as_ref()
                .with_context(|| format!("project `{}` requested but no projects configured", project))?;
            let spec = projects
                .get(project)
                .with_context(|| format!("unknown project `{}`", project))?;

            if let Some(chdir) = &spec.chdir {
                std::env::set_current_dir(chdir)
                    .with_context(|| format!("changing directory to project `{}` at {}", project, chdir))?;
            }
            project_language = spec.language.clone();
        }

        let tasks = TaskIndex::from_config(&config)?;
        Ok(Self {
            ctx,
            config_path,
            config_source,
            config,
            project_language,
            tasks,
        })
    }

    fn effective_language(&self, override_lang: Option<String>) -> Option<String> {
        self.ctx
            .effective_language(&self.config, self.project_language.as_deref(), override_lang)
    }

    fn env_path(&self) -> Result<Utf8PathBuf> {
        let cwd = envfile::current_working_dir()?;
        envfile::locate(&cwd)
    }
}
fn handle_language_set(ctx: &CliContext, name: String) -> Result<()> {
    let resolved = ctx.resolve_config_path()?;
    let path = resolved.path;
    config::set_default_language(&path, &name)?;
    println!(
        "Default language set to `{}` in {} ({})",
        name,
        path,
        resolved.source.as_str()
    );
    println!("Reload config to apply for this session.");
    Ok(())
}
