use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command as ProcessCommand, ExitStatus, Stdio};
use std::thread;
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use std::{fs, io};

use anyhow::{Context, Result, anyhow, bail};
use camino::Utf8PathBuf;
use clap::Parser;

use crate::cli::{
    AgentCommand, Cli, Command, ConfigCommand, DockerBuildArgs, DockerCommand,
    DockerComposeCommand, DockerComposeUpBuildArgs, DockerComposeUpCommand, DockerInitArgs,
    EnvArgs, EnvCommand, GitCommand, InstallArgs, LanguageCommand, OsCommand, ResearchCommand,
    ResearchInitArgs, SetupCommand, StartArgs, SummaryCommand, VaultCommand, Verb, VersionCommand,
};
use crate::config::{DevConfig, TaskUpdateMode};
use crate::envfile;
use crate::tasks::{CommandSpec, TaskIndex};
use crate::{config, dockergen, gitops, scaffold, vault, versioning};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ConfigPathSource {
    Explicit,
    Discovered,
    HomeDefault,
}

fn config_root_dir(config_path: &Utf8PathBuf) -> PathBuf {
    let p = Path::new(config_path.as_str());
    let parent = p.parent().unwrap_or(Path::new("."));

    if parent.file_name() == Some(std::ffi::OsStr::new(".dev")) {
        return parent.parent().unwrap_or(parent).to_path_buf();
    }

    if parent.file_name() == Some(std::ffi::OsStr::new("dev"))
        && let Some(tools) = parent.parent()
        && tools.file_name() == Some(std::ffi::OsStr::new("tools"))
    {
        return tools.parent().unwrap_or(tools).to_path_buf();
    }

    parent.to_path_buf()
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

struct WalkRequest {
    directory: PathBuf,
    output: PathBuf,
    max_depth: u32,
    no_content: bool,
    extensions: Option<Vec<String>>,
    include_hidden: bool,
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
        Command::Setup {
            command,
            skip_installed,
            no_deps,
        } => handle_setup(&ctx, command, skip_installed, no_deps),
        Command::Review {
            output,
            include_working,
            main,
        } => handle_review(&ctx, output, include_working, main),
        Command::Walk {
            directory,
            output,
            format: _format,
            max_depth,
            no_content,
            extensions,
            include_hidden,
        } => handle_walk(
            &ctx,
            WalkRequest {
                directory,
                output,
                max_depth,
                no_content,
                extensions,
                include_hidden,
            },
        ),
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
        Command::Docker { command } => handle_docker(state, command),
        Command::Research { command } => handle_research(state, command),
        Command::Vault { command } => handle_vault(state, command),
        Command::Os { command } => handle_os(state, command),
        Command::Config { .. } => unreachable!("config commands handled earlier"),
        Command::Setup { .. } => unreachable!("setup commands handled earlier"),
        Command::Review { .. } => unreachable!("review commands handled earlier"),
        Command::Walk { .. } => unreachable!("walk commands handled earlier"),
        Command::Summary { command } => handle_summary(state, command),
        Command::Agent { command } => handle_agent(state, command),
        Command::External(extra) => {
            bail!("unknown command: {}", extra.join(" "))
        }
    }
}

fn handle_research(state: &AppState, command: ResearchCommand) -> Result<()> {
    match command {
        ResearchCommand::Init(args) => research_init(state, args),
    }
}

fn research_init(state: &AppState, args: ResearchInitArgs) -> Result<()> {
    let target = if args.directory.is_absolute() {
        args.directory.clone()
    } else {
        std::env::current_dir()?.join(&args.directory)
    };
    let target = target.canonicalize().unwrap_or_else(|_| target.clone());

    let project_name = args.name.unwrap_or_else(|| {
        target
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| "research-project".to_owned())
    });
    let package_name = args
        .package
        .unwrap_or_else(|| normalize_package_name(&project_name));

    if target.exists() && target.read_dir()?.next().is_some() && !args.force {
        bail!(
            "target directory is not empty: {} (rerun with --force to overwrite scaffold files)",
            target.display()
        );
    }

    if state.ctx.dry_run {
        println!(
            "[dry-run] would initialize research project at {}",
            target.display()
        );
        println!("[dry-run] project name: {}", project_name);
        println!("[dry-run] package name: {}", package_name);
        if !args.skip_install {
            println!(
                "[dry-run] would run: uv add \"research-harness @ git+{}\"",
                args.harness_git
            );
        }
        return Ok(());
    }

    fs::create_dir_all(&target).with_context(|| format!("creating {}", target.display()))?;

    let project_yaml = format!(
        "name: {name}\nversion: \"0.1.0\"\ndescription: \"\"\n\nexperiments:\n  - id: example\n    module: experiments.example\n    callable: run\n    description: \"Example experiment\"\n\nconfig:\n  default: configs/default.yaml\n\ndatasets: []\n\nthresholds: configs/thresholds.yaml\n\noutputs:\n  format: parquet\n",
        name = project_name
    );

    let default_config = "# Default experiment configuration.\n";
    let thresholds = "thresholds: {}\n";
    let experiments_init = "\"\"\"Experiments package.\"\"\"\n";
    let example_exp = "\"\"\"Example experiment module.\"\"\"\n\n\ndef run(seed: int, output_dir, **kwargs):\n    \"\"\"Run the example experiment.\"\"\"\n    return {\"status\": \"ok\", \"seed\": seed}\n";
    let package_init = format!("\"\"\"Reusable package for {}.\"\"\"\n", project_name);
    let bindings_init = "\"\"\"Bindings for target systems.\"\"\"\n";
    let binding_example =
        "\"\"\"Example binding stub.\n\nImplement clean-room adapters here.\n\"\"\"\n";
    let analysis_tpl = "# Analysis Report\n\n## Scope\n- Hypothesis:\n- Dataset(s):\n- Config + seed policy:\n\n## Results\n- Run IDs:\n- Threshold outcomes:\n- Key observations:\n\n## Risks / Caveats\n-\n";
    let synthesis_tpl = "# Meta-Synthesis\n\n## Experiments Included\n-\n\n## Cross-Experiment Findings\n-\n\n## Plain-Language Overview\n-\n";
    let env_example = "HARNESS_HOME=.harness\n";
    let env_local = "HARNESS_HOME=.harness\n";

    write_scaffold_file(&target.join("project.yaml"), &project_yaml, args.force)?;
    write_scaffold_file(
        &target.join("configs").join("default.yaml"),
        default_config,
        args.force,
    )?;
    write_scaffold_file(
        &target.join("configs").join("thresholds.yaml"),
        thresholds,
        args.force,
    )?;
    write_scaffold_file(
        &target.join("experiments").join("__init__.py"),
        experiments_init,
        args.force,
    )?;
    write_scaffold_file(
        &target.join("experiments").join("example.py"),
        example_exp,
        args.force,
    )?;
    write_scaffold_file(
        &target.join("src").join(&package_name).join("__init__.py"),
        &package_init,
        args.force,
    )?;
    write_scaffold_file(
        &target
            .join("src")
            .join(&package_name)
            .join("bindings")
            .join("__init__.py"),
        bindings_init,
        args.force,
    )?;
    write_scaffold_file(
        &target
            .join("src")
            .join(&package_name)
            .join("bindings")
            .join("example.py"),
        binding_example,
        args.force,
    )?;
    write_scaffold_file(
        &target.join("reports").join("templates").join("analysis.md"),
        analysis_tpl,
        args.force,
    )?;
    write_scaffold_file(
        &target
            .join("reports")
            .join("templates")
            .join("meta_synthesis.md"),
        synthesis_tpl,
        args.force,
    )?;
    write_scaffold_file(
        &target.join(".harness").join("runs").join(".gitkeep"),
        "",
        args.force,
    )?;
    write_scaffold_file(
        &target.join(".harness").join("datasets").join(".gitkeep"),
        "",
        args.force,
    )?;
    write_scaffold_file(&target.join(".env.example"), env_example, args.force)?;
    write_scaffold_file(&target.join(".env"), env_local, false)?;

    if !args.skip_install {
        let argv = vec![
            "uv".to_owned(),
            "add".to_owned(),
            format!("research-harness @ git+{}", args.harness_git),
        ];
        println!("Installing harness dependency: {}", format_command(&argv));
        let status = run_process_streaming_in_dir(&argv, &target)?;
        if !status.success() {
            bail!(
                "command `{}` failed with exit code {:?}",
                format_command(&argv),
                status.code()
            );
        }
    }

    println!("Research project initialized at {}", target.display());
    println!("  project.yaml");
    println!("  configs/default.yaml");
    println!("  configs/thresholds.yaml");
    println!("  experiments/example.py");
    println!("  src/{}/bindings/", package_name);
    println!("  reports/templates/");
    println!("  .harness/ (project-local run state)");
    println!("  .env with HARNESS_HOME=.harness");

    Ok(())
}

fn normalize_package_name(input: &str) -> String {
    let mut out = String::new();
    let mut prev_was_sep = false;
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            out.push(ch.to_ascii_lowercase());
            prev_was_sep = false;
        } else if !prev_was_sep {
            out.push('_');
            prev_was_sep = true;
        }
    }
    let trimmed = out.trim_matches('_').to_string();
    let mut final_name = if trimmed.is_empty() {
        "research_project".to_owned()
    } else {
        trimmed
    };
    if final_name
        .chars()
        .next()
        .map(|c| c.is_ascii_digit())
        .unwrap_or(false)
    {
        final_name = format!("pkg_{}", final_name);
    }
    final_name
}

fn write_scaffold_file(path: &Path, content: &str, force: bool) -> Result<()> {
    if path.exists() && !force {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }
    fs::write(path, content).with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

fn handle_docker(state: &AppState, command: DockerCommand) -> Result<()> {
    match command {
        DockerCommand::Init(args) => docker_init(state, args),
        DockerCommand::Build(args) => docker_build(state, args),
        DockerCommand::Compose { command } => docker_compose(state, command),
        DockerCommand::Develop(args) => docker_develop(state, args),
    }
}

fn docker_develop(state: &AppState, args: crate::cli::DockerDevelopArgs) -> Result<()> {
    if !args.no_up {
        let argv = vec![
            "docker".to_owned(),
            "compose".to_owned(),
            "up".to_owned(),
            "-d".to_owned(),
            "--build".to_owned(),
        ];
        println!("Starting compose service: {}", format_command(&argv));
        if state.ctx.dry_run {
            println!("    (dry-run) skipped");
        } else {
            let status = run_process(&argv)?;
            if !status.success() {
                bail!(
                    "command `{}` failed with exit code {:?}",
                    format_command(&argv),
                    status.code()
                );
            }
        }
    }

    println!("Opening shell in service `{}`...", args.service);
    if state.ctx.dry_run {
        println!("[dry-run] docker compose exec {} bash -l", args.service);
        return Ok(());
    }

    let status = ProcessCommand::new("docker")
        .arg("compose")
        .arg("exec")
        .arg(&args.service)
        .arg("bash")
        .arg("-l")
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .with_context(|| format!("executing `docker compose exec {} bash -l`", args.service))?;

    if status.success() {
        Ok(())
    } else {
        bail!("docker compose exec exited with code {:?}", status.code())
    }
}

fn docker_init(state: &AppState, args: DockerInitArgs) -> Result<()> {
    dockergen::init(&args, state.ctx.dry_run)
}

fn docker_build(state: &AppState, args: DockerBuildArgs) -> Result<()> {
    let image = match args.image.as_deref() {
        Some(value) if !value.trim().is_empty() => value.trim().to_owned(),
        _ => resolve_core_image_from_env()?,
    };

    let argv = vec![
        "docker".to_owned(),
        "build".to_owned(),
        "-f".to_owned(),
        "docker/Dockerfile.core".to_owned(),
        "-t".to_owned(),
        image,
        ".".to_owned(),
    ];

    println!("Building core image: {}", format_command(&argv));
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

fn docker_compose(state: &AppState, command: DockerComposeCommand) -> Result<()> {
    match command {
        DockerComposeCommand::Up { command } => docker_compose_up(state, command),
    }
}

fn docker_compose_up(state: &AppState, command: DockerComposeUpCommand) -> Result<()> {
    match command {
        DockerComposeUpCommand::Build(args) => docker_compose_up_build(state, args),
    }
}

fn docker_compose_up_build(state: &AppState, args: DockerComposeUpBuildArgs) -> Result<()> {
    let mut argv = vec![
        "docker".to_owned(),
        "compose".to_owned(),
        "up".to_owned(),
        "--build".to_owned(),
    ];
    if args.detach {
        argv.push("-d".to_owned());
    }

    println!("Running compose: {}", format_command(&argv));
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

fn resolve_core_image_from_env() -> Result<String> {
    let cwd = envfile::current_working_dir()?;
    let env_path = envfile::locate(&cwd)?;
    let file = envfile::EnvFile::load(&env_path)?;

    for (key, value) in file.entries() {
        if key == "CORE_IMAGE" {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                bail!("CORE_IMAGE is empty in {}", env_path);
            }
            return Ok(trimmed.to_owned());
        }
    }

    Ok("devkit-core:local".to_owned())
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

    let port = args.port.or(if args.prod { Some(8091) } else { None });
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
    run_task_sequence_summarized(state, &tasks)
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
        run_task_sequence_summarized(state, &tasks)?;
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
        if args.no_scaffold {
            println!(
                "[dry-run] would install tooling and provisioning commands for `{}` without scaffolds",
                language
            );
        } else {
            println!(
                "[dry-run] would install scaffolds and tooling for `{}`",
                language
            );
        }
        return Ok(());
    }

    if args.no_scaffold {
        println!("Installing tooling for `{}` without scaffolds...", language);
        scaffold::install_tools(&language)?;
    } else {
        println!("Installing scaffolds for `{}`...", language);
        scaffold::install(&language, args.force)?;
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

fn handle_vault(state: &AppState, command: VaultCommand) -> Result<()> {
    match command {
        VaultCommand::List { account } => vault::list_items(&account, state.ctx.dry_run),
        VaultCommand::Get {
            item,
            field,
            account,
        } => vault::get_item(&account, &item, field.as_deref(), state.ctx.dry_run),
        VaultCommand::Set {
            item,
            value,
            account,
        } => vault::set_item(&account, &item, &value, state.ctx.dry_run),
        VaultCommand::Delete { item, account } => {
            vault::delete_item(&account, &item, state.ctx.dry_run)
        }
    }
}

fn handle_os(state: &AppState, command: OsCommand) -> Result<()> {
    let config_path = &state.config_path;

    match command {
        OsCommand::Show => {
            let current_os = std::env::consts::OS;
            println!("Detected OS: {}", current_os);
            println!(
                "Config path: {} ({})",
                config_path,
                state.config_source.as_str()
            );

            let config_dir = config_path
                .parent()
                .ok_or_else(|| anyhow!("cannot determine config directory"))?;

            let linux_path = config_dir.join("config.linux.toml");
            let windows_path = config_dir.join("config.windows.toml");

            println!(
                "Linux config:   {} ({})",
                linux_path,
                if linux_path.exists() {
                    "exists"
                } else {
                    "not found"
                }
            );
            println!(
                "Windows config: {} ({})",
                windows_path,
                if windows_path.exists() {
                    "exists"
                } else {
                    "not found"
                }
            );

            Ok(())
        }
        OsCommand::Linux => os_switch(state, "linux"),
        OsCommand::Windows => os_switch(state, "windows"),
    }
}

fn os_switch(state: &AppState, target_os: &str) -> Result<()> {
    let template_path = format!("config/tauri.config.{}.toml", target_os);
    let content = crate::templates::get_string(&template_path)
        .with_context(|| format!("loading embedded template for {}", target_os))?;

    let config_dir = state
        .config_path
        .parent()
        .ok_or_else(|| anyhow!("cannot determine config directory"))?;

    let platform_config = config_dir.join(format!("config.{}.toml", target_os));

    if state.ctx.dry_run {
        println!(
            "[dry-run] would write {} config to {}",
            target_os, platform_config
        );
        return Ok(());
    }

    fs::create_dir_all(config_dir.as_std_path())
        .with_context(|| format!("creating directory {}", config_dir))?;

    fs::write(platform_config.as_std_path(), &content)
        .with_context(|| format!("writing {}", platform_config))?;

    println!("Wrote {} config to {}", target_os, platform_config);
    println!(
        "This config will be preferred when running on {}.",
        target_os
    );

    Ok(())
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

    println!(
        "Switched to profile `{}` (copied {} to {})",
        profile, profile_path, env_path
    );
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

    println!(
        "Saved current .env as profile `{}` at {}",
        name, profile_path
    );
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
                let value = env
                    .entries()
                    .find(|(k, _)| k == key)
                    .map(|(_, v)| v)
                    .unwrap_or("");
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
    let ref_keys: std::collections::HashSet<_> =
        ref_env.entries().map(|(k, _)| k.to_owned()).collect();

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
    let ref_keys: std::collections::HashSet<_> =
        ref_env.entries().map(|(k, _)| k.to_owned()).collect();

    let missing: Vec<_> = ref_keys.difference(&env_keys).cloned().collect();

    if missing.is_empty() {
        println!(
            "No missing keys. {} is in sync with {}.",
            env_path, ref_path
        );
        return Ok(());
    }

    println!("Adding {} missing keys from {}:", missing.len(), reference);
    for key in &missing {
        let value = ref_env
            .entries()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v)
            .unwrap_or("");
        env.upsert(key, value);
        println!(
            "  + {}={}",
            key,
            if value.is_empty() { "(empty)" } else { "*****" }
        );
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
            println!(
                "Config path: {} ({})",
                config_path,
                resolved.source.as_str()
            );
            Ok(())
        }
        None | Some(ConfigCommand::Show) => {
            if !config_path.exists() {
                println!("No config found at {}.", config_path);
                println!("Use `dev config generate` to scaffold a default configuration.");
                return Ok(());
            }

            let config = config::load_from_path(&config_path)?;
            println!(
                "Config path: {} ({})",
                config_path,
                resolved.source.as_str()
            );
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
            println!(
                "Reloaded config from {} ({})",
                config_path,
                resolved.source.as_str()
            );
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
    use std::sync::{Mutex, OnceLock};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn cwd_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

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
        let _guard = cwd_lock().lock().unwrap();
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
        let _guard = cwd_lock().lock().unwrap();
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
        let _guard = cwd_lock().lock().unwrap();
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
        assert_eq!(
            state.effective_language(None).as_deref(),
            Some("typescript")
        );

        std::env::set_current_dir(old).unwrap();
        let _ = fs::remove_dir_all(root.as_std_path());
    }
}

fn run_task_sequence_summarized(state: &AppState, tasks: &[String]) -> Result<()> {
    for task in tasks {
        println!("Running summarized task `{}`", task);
        let commands = state.tasks.flatten(task)?;
        execute_commands_summarized(state, task, &commands, false)?;
    }
    Ok(())
}

#[derive(Debug)]
struct AgentLaunch {
    argv: Vec<String>,
    cwd: PathBuf,
    prompt: String,
    model: Option<String>,
    prompt_stdin: bool,
    iterations: u32,
}

fn handle_agent(state: &AppState, command: AgentCommand) -> Result<()> {
    match command {
        AgentCommand::Run(args) => handle_agent_run(state, args),
        AgentCommand::List => handle_agent_list(),
        AgentCommand::Status(args) => handle_agent_status(&args.job_id, args.tail),
    }
}

fn handle_agent_run(state: &AppState, args: crate::cli::AgentRunArgs) -> Result<()> {
    let agent_name = if args.agent == "default" {
        state
            .config
            .default_agent
            .clone()
            .unwrap_or_else(|| args.agent.clone())
    } else {
        args.agent.clone()
    };

    let agents = state
        .config
        .agents
        .as_ref()
        .ok_or_else(|| anyhow!("no agents configured; add an [agents.{agent_name}] table"))?;
    let agent = agents
        .get(&agent_name)
        .with_context(|| format!("unknown agent `{}`", agent_name))?;

    let prompt = read_agent_prompt(args.prompt.as_deref(), args.prompt_file.as_ref())?;
    if prompt.trim().is_empty() {
        bail!("agent prompt cannot be empty");
    }

    let cwd = resolve_agent_cwd(agent.cwd.as_deref(), args.cwd.as_ref())?;
    let model = args.model.or_else(|| agent.model.clone());
    let launch = build_agent_launch(
        agent,
        &cwd,
        model,
        prompt,
        &args.extra_args,
        args.iterations,
    )?;

    println!(
        "Launching agent `{}`: {}",
        agent_name,
        format_command(&launch.argv)
    );
    println!("  cwd: {}", launch.cwd.display());
    if let Some(model) = &launch.model {
        println!("  model: {}", model);
    }
    if launch.iterations > 1 {
        println!("  iterations: {}", launch.iterations);
    }

    if state.ctx.dry_run {
        println!("    (dry-run) skipped");
        return Ok(());
    }

    if args.attach {
        launch_agent_foreground(launch)
    } else {
        launch_agent_detached(&agent_name, launch)
    }
}

#[derive(Debug)]
struct AgentJobRecord {
    id: String,
    agent: String,
    pid: u32,
    log_path: PathBuf,
    command: String,
    model: Option<String>,
    cwd: PathBuf,
    iterations: u32,
    started_at: u64,
}

fn handle_agent_list() -> Result<()> {
    let dir = agent_jobs_dir()?;
    if !dir.exists() {
        println!("No agent jobs found.");
        return Ok(());
    }

    let mut jobs = Vec::new();
    for entry in fs::read_dir(&dir).with_context(|| format!("reading {}", dir.display()))? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("job") {
            continue;
        }
        if let Ok(record) = read_agent_job(&path) {
            jobs.push(record);
        }
    }

    jobs.sort_by_key(|job| job.started_at);
    if jobs.is_empty() {
        println!("No agent jobs found.");
        return Ok(());
    }

    for job in jobs {
        let state = if process_is_running(job.pid) {
            "running"
        } else {
            "exited"
        };
        println!(
            "{}  {}  pid={}  agent={}  model={}  log={}",
            job.id,
            state,
            job.pid,
            job.agent,
            job.model.as_deref().unwrap_or("<none>"),
            job.log_path.display()
        );
    }
    Ok(())
}

fn handle_agent_status(job_id: &str, tail: usize) -> Result<()> {
    let path = agent_jobs_dir()?.join(format!("{}.job", job_id));
    let job = read_agent_job(&path).with_context(|| format!("reading job `{}`", job_id))?;
    let running = process_is_running(job.pid);
    println!("Job: {}", job.id);
    println!("Agent: {}", job.agent);
    println!("State: {}", if running { "running" } else { "exited" });
    println!("PID: {}", job.pid);
    println!("Model: {}", job.model.as_deref().unwrap_or("<none>"));
    println!("CWD: {}", job.cwd.display());
    println!("Iterations: {}", job.iterations);
    println!("Command: {}", job.command);
    println!("Log: {}", job.log_path.display());

    let log = fs::read_to_string(&job.log_path).unwrap_or_default();
    if log.trim().is_empty() {
        println!("Summary\n- log is empty");
        return Ok(());
    }
    let lines = log.lines().rev().take(tail).collect::<Vec<_>>();
    println!("Summary");
    for line in lines.into_iter().rev() {
        let lowered = line.to_ascii_lowercase();
        if lowered.contains("error")
            || lowered.contains("failed")
            || lowered.contains("complete")
            || lowered.contains("done")
            || lowered.contains("iteration")
            || lowered.contains("summary")
        {
            println!("- {}", line);
        }
    }
    println!("Tail");
    for line in log
        .lines()
        .rev()
        .take(tail)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
    {
        println!("{}", line);
    }
    Ok(())
}

fn read_agent_prompt(prompt: Option<&str>, prompt_file: Option<&PathBuf>) -> Result<String> {
    match (prompt, prompt_file) {
        (Some(_), Some(_)) => bail!("use either --prompt or --prompt-file, not both"),
        (Some(value), None) => Ok(value.to_owned()),
        (None, Some(path)) if path == Path::new("-") => {
            let mut input = String::new();
            std::io::Read::read_to_string(&mut io::stdin(), &mut input)
                .context("reading prompt from stdin")?;
            Ok(input)
        }
        (None, Some(path)) => {
            fs::read_to_string(path).with_context(|| format!("reading prompt {}", path.display()))
        }
        (None, None) => bail!("provide --prompt or --prompt-file"),
    }
}

fn resolve_agent_cwd(configured: Option<&str>, override_cwd: Option<&PathBuf>) -> Result<PathBuf> {
    let cwd = if let Some(path) = override_cwd {
        path.clone()
    } else if let Some(path) = configured {
        PathBuf::from(path)
    } else {
        std::env::current_dir().context("determining current directory")?
    };

    if cwd.is_absolute() {
        Ok(cwd)
    } else {
        Ok(std::env::current_dir()
            .context("determining current directory")?
            .join(cwd))
    }
}

fn build_agent_launch(
    agent: &crate::config::AgentConfig,
    cwd: &Path,
    model: Option<String>,
    prompt: String,
    extra_args: &[String],
    iteration_override: Option<u32>,
) -> Result<AgentLaunch> {
    let prompt = match agent.prompt_prefix.as_deref() {
        Some(prefix) if !prefix.trim().is_empty() => format!("{}\n\n{}", prefix, prompt),
        _ => prompt,
    };
    let mut adapter_args = agent.extra_args.clone().unwrap_or_default();
    adapter_args.extend(extra_args.iter().cloned());
    let adapter = agent.adapter.as_deref().unwrap_or("codex");
    let iterations = iteration_override.or(agent.iterations).unwrap_or(1);
    if iterations == 0 {
        bail!("agent iterations must be greater than zero");
    }

    let argv = match adapter {
        "codex" => {
            let mut argv = vec!["codex".to_owned(), "exec".to_owned()];
            if let Some(model) = &model {
                argv.push("--model".to_owned());
                argv.push(model.clone());
            }
            argv.push("--cd".to_owned());
            argv.push(cwd.display().to_string());
            argv.extend(adapter_args);
            argv.push(prompt.clone());
            argv
        }
        "command" | "generic" | "loop" => {
            let command = agent
                .command
                .clone()
                .ok_or_else(|| anyhow!("{} agent adapter requires `command`", adapter))?;
            if command.is_empty() {
                bail!("{} agent command cannot be empty", adapter);
            }
            let mut argv = command;
            argv.extend(adapter_args);
            argv
        }
        other => bail!("unsupported agent adapter `{}`", other),
    };

    Ok(AgentLaunch {
        argv,
        cwd: cwd.to_path_buf(),
        prompt,
        model,
        prompt_stdin: matches!(adapter, "command" | "generic" | "loop"),
        iterations: if adapter == "loop" { iterations } else { 1 },
    })
}

fn launch_agent_foreground(launch: AgentLaunch) -> Result<()> {
    for iteration in 1..=launch.iterations {
        if launch.iterations > 1 {
            println!(
                "[{}/{}] {}",
                iteration,
                launch.iterations,
                format_command(&launch.argv)
            );
        }
        launch_agent_foreground_once(&launch, iteration)?;
    }
    Ok(())
}

fn launch_agent_foreground_once(launch: &AgentLaunch, iteration: u32) -> Result<()> {
    let mut command = ProcessCommand::new(&launch.argv[0]);
    command.args(&launch.argv[1..]);
    command.current_dir(&launch.cwd);
    command.env("DEV_AGENT_PROMPT", &launch.prompt);
    command.env("DEV_AGENT_CWD", &launch.cwd);
    command.env("DEV_AGENT_ITERATION", iteration.to_string());
    command.env("DEV_AGENT_ITERATIONS", launch.iterations.to_string());
    if let Some(model) = &launch.model {
        command.env("DEV_AGENT_MODEL", model);
    }
    if launch.prompt_stdin {
        command.stdin(Stdio::piped());
    } else {
        command.stdin(Stdio::inherit());
    }
    command.stdout(Stdio::inherit());
    command.stderr(Stdio::inherit());

    let mut child = command
        .spawn()
        .with_context(|| format!("launching agent `{}`", format_command(&launch.argv)))?;
    if launch.prompt_stdin
        && let Some(mut stdin) = child.stdin.take()
    {
        stdin
            .write_all(launch.prompt.as_bytes())
            .context("writing prompt to agent stdin")?;
    }

    let status = child.wait().context("waiting for agent")?;
    if status.success() {
        Ok(())
    } else {
        bail!(
            "agent iteration {}/{} exited with code {:?}",
            iteration,
            launch.iterations,
            status.code()
        )
    }
}

fn launch_agent_detached(agent_name: &str, launch: AgentLaunch) -> Result<()> {
    if launch.iterations > 1 {
        println!("Async loop iterations: {}", launch.iterations);
    }
    let started_at = unix_timestamp()?;
    let job_id = format!("{}-{}", started_at, safe_agent_name(agent_name));
    let log_path = agent_log_path(&job_id)?;
    let stdout = fs::File::create(&log_path)
        .with_context(|| format!("creating agent log {}", log_path.display()))?;
    let stderr = stdout
        .try_clone()
        .with_context(|| format!("cloning agent log {}", log_path.display()))?;

    let mut command = ProcessCommand::new(&launch.argv[0]);
    command.args(&launch.argv[1..]);
    command.current_dir(&launch.cwd);
    command.env("DEV_AGENT_PROMPT", &launch.prompt);
    command.env("DEV_AGENT_CWD", &launch.cwd);
    command.env("DEV_AGENT_ITERATIONS", launch.iterations.to_string());
    if let Some(model) = &launch.model {
        command.env("DEV_AGENT_MODEL", model);
    }
    if launch.iterations > 1 {
        let loop_script = detached_loop_script(&launch.argv, launch.iterations);
        command = ProcessCommand::new("bash");
        command.arg("-lc").arg(loop_script);
        command.current_dir(&launch.cwd);
        command.env("DEV_AGENT_PROMPT", &launch.prompt);
        command.env("DEV_AGENT_CWD", &launch.cwd);
        command.env("DEV_AGENT_ITERATIONS", launch.iterations.to_string());
        if let Some(model) = &launch.model {
            command.env("DEV_AGENT_MODEL", model);
        }
    }
    if launch.prompt_stdin {
        command.stdin(Stdio::piped());
    } else {
        command.stdin(Stdio::null());
    }
    command.stdout(Stdio::from(stdout));
    command.stderr(Stdio::from(stderr));

    let mut child = command.spawn().with_context(|| {
        format!(
            "launching detached agent `{}`",
            format_command(&launch.argv)
        )
    })?;
    if launch.prompt_stdin
        && launch.iterations == 1
        && let Some(mut stdin) = child.stdin.take()
    {
        stdin
            .write_all(launch.prompt.as_bytes())
            .context("writing prompt to detached agent stdin")?;
    }
    let record = AgentJobRecord {
        id: job_id,
        agent: agent_name.to_owned(),
        pid: child.id(),
        log_path: log_path.clone(),
        command: format_command(&launch.argv),
        model: launch.model.clone(),
        cwd: launch.cwd.clone(),
        iterations: launch.iterations,
        started_at,
    };
    write_agent_job(&record)?;
    println!("Agent job: {}", record.id);
    println!("PID: {}", child.id());
    println!("Log: {}", log_path.display());
    println!("Check later: dev agent status {}", record.id);
    Ok(())
}

fn detached_loop_script(argv: &[String], iterations: u32) -> String {
    let command = shell_command(argv);
    let display = shell_single_quote(&format_command(argv));
    format!(
        "for i in $(seq 1 {iterations}); do printf '[iteration %s/{iterations}] %s\\n' \"$i\" {display}; printf '%s' \"$DEV_AGENT_PROMPT\" | DEV_AGENT_ITERATION=\"$i\" {command}; status=$?; if [ $status -ne 0 ]; then exit $status; fi; done",
    )
}

fn shell_single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn agent_log_path(job_id: &str) -> Result<PathBuf> {
    let home = dirs::home_dir().context("determining home directory")?;
    let dir = home.join(".dev").join("agents");
    fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
    Ok(dir.join(format!("{}.log", job_id)))
}

fn agent_jobs_dir() -> Result<PathBuf> {
    let home = dirs::home_dir().context("determining home directory")?;
    let dir = home.join(".dev").join("agents").join("jobs");
    fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
    Ok(dir)
}

fn unix_timestamp() -> Result<u64> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock before unix epoch")?
        .as_secs())
}

fn safe_agent_name(agent_name: &str) -> String {
    agent_name
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>()
}

fn write_agent_job(record: &AgentJobRecord) -> Result<()> {
    let path = agent_jobs_dir()?.join(format!("{}.job", record.id));
    let mut out = String::new();
    out.push_str(&format!("id={}\n", record.id));
    out.push_str(&format!("agent={}\n", record.agent));
    out.push_str(&format!("pid={}\n", record.pid));
    out.push_str(&format!("log_path={}\n", record.log_path.display()));
    out.push_str(&format!(
        "command={}\n",
        record.command.replace('\n', "\\n")
    ));
    out.push_str(&format!(
        "model={}\n",
        record.model.as_deref().unwrap_or("")
    ));
    out.push_str(&format!("cwd={}\n", record.cwd.display()));
    out.push_str(&format!("iterations={}\n", record.iterations));
    out.push_str(&format!("started_at={}\n", record.started_at));
    fs::write(&path, out).with_context(|| format!("writing {}", path.display()))
}

fn read_agent_job(path: &Path) -> Result<AgentJobRecord> {
    let raw = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let mut values = std::collections::BTreeMap::new();
    for line in raw.lines() {
        if let Some((key, value)) = line.split_once('=') {
            values.insert(key.to_owned(), value.to_owned());
        }
    }
    let get = |key: &str| {
        values
            .get(key)
            .cloned()
            .ok_or_else(|| anyhow!("job file missing `{}`", key))
    };
    let model = get("model")?;
    Ok(AgentJobRecord {
        id: get("id")?,
        agent: get("agent")?,
        pid: get("pid")?.parse().context("parsing job pid")?,
        log_path: PathBuf::from(get("log_path")?),
        command: get("command")?.replace("\\n", "\n"),
        model: if model.is_empty() { None } else { Some(model) },
        cwd: PathBuf::from(get("cwd")?),
        iterations: get("iterations")?.parse().context("parsing iterations")?,
        started_at: get("started_at")?.parse().context("parsing started_at")?,
    })
}

fn process_is_running(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    ProcessCommand::new("kill")
        .arg("-0")
        .arg(pid.to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

#[derive(Debug)]
struct CapturedCommandResult {
    status: ExitStatus,
    stdout: String,
    stderr: String,
    elapsed: std::time::Duration,
}

#[derive(Debug)]
struct SummaryOptions {
    shell: String,
    max_output_bytes: usize,
    tail_bytes: usize,
    llm_command: Option<String>,
}

fn handle_summary(state: &AppState, command: SummaryCommand) -> Result<()> {
    match command {
        SummaryCommand::Run(args) => handle_summary_run(state, &args.task, args.raw),
        SummaryCommand::Exec(args) => handle_summary_exec(state, &args.argv, args.raw),
    }
}

fn handle_summary_run(state: &AppState, task: &str, raw: bool) -> Result<()> {
    println!("Running summarized task `{}`", task);
    let commands = state.tasks.flatten(task)?;
    execute_commands_summarized(state, task, &commands, raw)
}

fn handle_summary_exec(state: &AppState, argv: &[String], raw: bool) -> Result<()> {
    if argv.is_empty() {
        bail!("summary exec requires a command after `--`");
    }
    let spec = CommandSpec {
        origin: "exec".to_owned(),
        argv: argv.to_owned(),
        allow_fail: false,
    };
    execute_commands_summarized(state, "exec", &[spec], raw)
}

fn execute_commands_summarized(
    state: &AppState,
    task: &str,
    commands: &[CommandSpec],
    raw: bool,
) -> Result<()> {
    if commands.is_empty() {
        println!("Task `{}` has no commands.", task);
        return Ok(());
    }

    let options = summary_options(&state.config);
    let total = commands.len();
    for (idx, spec) in commands.iter().enumerate() {
        let render = format_command(&spec.argv);
        println!(
            "[{}/{}] {} :: {} (shell: {})",
            idx + 1,
            total,
            spec.origin,
            render,
            options.shell
        );

        if state.ctx.dry_run {
            println!("    (dry-run) skipped");
            continue;
        }

        let result = run_process_captured_in_shell(&spec.argv, &options.shell)?;
        let combined = combine_output(&result.stdout, &result.stderr);
        let summary = summarize_captured_output(&render, &result, &combined, &options)?;
        println!("{}", summary);

        if raw {
            print_raw_output(&result.stdout, &result.stderr);
        }

        if result.status.success() {
            println!("[ok] {} (completed in {:.2?})", render, result.elapsed);
        } else if spec.allow_fail {
            println!(
                "[warn] {} failed with exit code {:?} (ignored)",
                render,
                result.status.code()
            );
        } else {
            bail!(
                "command `{}` failed with exit code {:?}",
                render,
                result.status.code()
            );
        }
    }

    if state.ctx.dry_run {
        println!("Task `{}` simulated (dry-run).", task);
    } else {
        println!("Task `{}` completed.", task);
    }

    Ok(())
}

fn summary_options(config: &DevConfig) -> SummaryOptions {
    let summary = config.summary.as_ref();
    let shell = std::env::var("DEV_SUMMARY_SHELL")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| summary.and_then(|s| s.shell.clone()))
        .unwrap_or_else(|| "bash".to_owned());
    let max_output_bytes = std::env::var("DEV_SUMMARY_MAX_OUTPUT_BYTES")
        .ok()
        .and_then(|value| value.parse().ok())
        .or_else(|| summary.and_then(|s| s.max_output_bytes))
        .unwrap_or(64 * 1024);
    let tail_bytes = std::env::var("DEV_SUMMARY_TAIL_BYTES")
        .ok()
        .and_then(|value| value.parse().ok())
        .or_else(|| summary.and_then(|s| s.tail_bytes))
        .unwrap_or(12 * 1024);
    let llm_command = std::env::var("DEV_SUMMARY_LLM_COMMAND")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| summary.and_then(|s| s.llm_command.clone()));

    SummaryOptions {
        shell,
        max_output_bytes,
        tail_bytes,
        llm_command,
    }
}

fn run_process_captured_in_shell(argv: &[String], shell: &str) -> Result<CapturedCommandResult> {
    let script = shell_command(argv);
    let start = Instant::now();
    let output = ProcessCommand::new(shell)
        .arg("-lc")
        .arg(&script)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .with_context(|| format!("executing `{}` through shell `{}`", script, shell))?;

    Ok(CapturedCommandResult {
        status: output.status,
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        elapsed: start.elapsed(),
    })
}

fn summarize_captured_output(
    command: &str,
    result: &CapturedCommandResult,
    combined: &str,
    options: &SummaryOptions,
) -> Result<String> {
    let bounded = bounded_output(combined, options.max_output_bytes, options.tail_bytes);
    if let Some(llm_command) = &options.llm_command
        && !bounded.trim().is_empty()
    {
        match summarize_with_llm_command(command, result, &bounded, llm_command) {
            Ok(summary) if !summary.trim().is_empty() => return Ok(summary),
            Ok(_) => {}
            Err(err) => {
                println!("[warn] LLM summary command failed: {err:#}");
            }
        }
    }

    Ok(local_summary(command, result, &bounded))
}

fn summarize_with_llm_command(
    command: &str,
    result: &CapturedCommandResult,
    output: &str,
    llm_command: &str,
) -> Result<String> {
    let prompt = format!(
        "Summarize this developer command result for another coding agent.\n\
         Be concise. Include the exit status, likely root cause, and next action.\n\
         Do not reproduce long traces.\n\n\
         Command: {command}\n\
         Exit code: {:?}\n\
         Duration: {:.2?}\n\n\
         Captured output:\n{output}\n",
        result.status.code(),
        result.elapsed
    );

    let mut child = ProcessCommand::new("bash")
        .arg("-lc")
        .arg(llm_command)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("starting LLM summary command `{}`", llm_command))?;

    if let Some(stdin) = child.stdin.as_mut() {
        stdin
            .write_all(prompt.as_bytes())
            .context("writing prompt to LLM summary command")?;
    }

    let output = child
        .wait_with_output()
        .context("waiting for LLM summary command")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "LLM summary command exited with {:?}: {}",
            output.status.code(),
            stderr.trim()
        );
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn local_summary(command: &str, result: &CapturedCommandResult, output: &str) -> String {
    let mut lines = output.lines();
    let interesting = output
        .lines()
        .filter(|line| {
            let lowered = line.to_ascii_lowercase();
            lowered.contains("error")
                || lowered.contains("failed")
                || lowered.contains("panic")
                || lowered.contains("exception")
                || lowered.contains("warning")
                || lowered.contains("traceback")
        })
        .take(12)
        .collect::<Vec<_>>();

    let preview = if interesting.is_empty() {
        lines.by_ref().rev().take(12).collect::<Vec<_>>()
    } else {
        interesting
    };

    let mut out = String::new();
    out.push_str("Summary\n");
    out.push_str(&format!("- command: {}\n", command));
    out.push_str(&format!("- exit: {:?}\n", result.status.code()));
    out.push_str(&format!("- duration: {:.2?}\n", result.elapsed));
    if output.trim().is_empty() {
        out.push_str("- output: <empty>\n");
    } else {
        out.push_str("- notable output:\n");
        for line in preview {
            out.push_str("  ");
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

fn bounded_output(output: &str, max_output_bytes: usize, tail_bytes: usize) -> String {
    if output.len() <= max_output_bytes {
        return output.to_owned();
    }

    let keep_head = max_output_bytes.saturating_sub(tail_bytes);
    let head = clamp_to_char_boundary(output, keep_head);
    let tail_start = output.len().saturating_sub(tail_bytes);
    let tail_start = clamp_start_to_char_boundary(output, tail_start);
    format!(
        "{}\n\n[... omitted {} bytes ...]\n\n{}",
        &output[..head],
        output
            .len()
            .saturating_sub(head + (output.len() - tail_start)),
        &output[tail_start..]
    )
}

fn clamp_to_char_boundary(value: &str, mut index: usize) -> usize {
    index = index.min(value.len());
    while index > 0 && !value.is_char_boundary(index) {
        index -= 1;
    }
    index
}

fn clamp_start_to_char_boundary(value: &str, mut index: usize) -> usize {
    index = index.min(value.len());
    while index < value.len() && !value.is_char_boundary(index) {
        index += 1;
    }
    index
}

fn combine_output(stdout: &str, stderr: &str) -> String {
    match (stdout.trim().is_empty(), stderr.trim().is_empty()) {
        (true, true) => String::new(),
        (false, true) => format!("stdout:\n{}", stdout),
        (true, false) => format!("stderr:\n{}", stderr),
        (false, false) => format!("stdout:\n{}\nstderr:\n{}", stdout, stderr),
    }
}

fn print_raw_output(stdout: &str, stderr: &str) {
    if !stdout.is_empty() {
        println!("Raw stdout\n{}", stdout);
    }
    if !stderr.is_empty() {
        println!("Raw stderr\n{}", stderr);
    }
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

fn shell_command(argv: &[String]) -> String {
    argv.iter()
        .map(|arg| {
            if arg.is_empty() {
                "''".to_owned()
            } else if arg.chars().all(|c| {
                c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '/' | ':' | '+')
            }) {
                arg.clone()
            } else {
                format!("'{}'", arg.replace('\'', "'\\''"))
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

fn strip_compose_container_name(path: &Path) -> Result<bool> {
    let content = match std::fs::read_to_string(path) {
        Ok(value) => value,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(err) => return Err(err).with_context(|| format!("reading {}", path.display())),
    };

    let mut changed = false;
    let mut out = String::with_capacity(content.len());
    for line in content.lines() {
        if line.trim_start().starts_with("container_name:") {
            changed = true;
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }

    if !changed {
        return Ok(false);
    }

    std::fs::write(path, out).with_context(|| format!("writing {}", path.display()))?;
    Ok(true)
}

fn run_process_streaming_in_dir(argv: &[String], cwd: &Path) -> Result<std::process::ExitStatus> {
    let mut command = ProcessCommand::new(&argv[0]);
    if argv.len() > 1 {
        command.args(&argv[1..]);
    }
    command.current_dir(cwd);
    command.stdout(Stdio::piped()).stderr(Stdio::piped());

    let mut child = command
        .spawn()
        .with_context(|| format!("executing `{}` in {}", format_command(argv), cwd.display()))?;

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
        /// Return the platform suffix for config file selection.
        fn platform_config_suffix() -> &'static str {
            match std::env::consts::OS {
                "linux" => "linux",
                "windows" => "windows",
                "macos" => "macos",
                _ => "linux",
            }
        }

        if let Some(path) = &self.file {
            let path = Utf8PathBuf::from_path_buf(path.clone())
                .map_err(|_| anyhow!("config path must be valid UTF-8"))?;
            return Ok(ResolvedConfigPath {
                path,
                source: ConfigPathSource::Explicit,
            });
        }

        if let Ok(cwd) = std::env::current_dir()
            && let Ok(mut dir) = Utf8PathBuf::from_path_buf(cwd)
        {
            loop {
                let platform_suffix = platform_config_suffix();

                // .dev/ path — platform-specific first, then generic
                let platform_preferred = dir
                    .join(".dev")
                    .join(format!("config.{}.toml", platform_suffix));
                if platform_preferred.exists() {
                    return Ok(ResolvedConfigPath {
                        path: platform_preferred,
                        source: ConfigPathSource::Discovered,
                    });
                }

                let preferred = dir.join(".dev").join("config.toml");
                if preferred.exists() {
                    return Ok(ResolvedConfigPath {
                        path: preferred,
                        source: ConfigPathSource::Discovered,
                    });
                }

                // Legacy tools/dev/ path — same priority
                let legacy_platform = dir
                    .join("tools")
                    .join("dev")
                    .join(format!("config.{}.toml", platform_suffix));
                if legacy_platform.exists() {
                    return Ok(ResolvedConfigPath {
                        path: legacy_platform,
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

        let home = dirs::home_dir().ok_or_else(|| anyhow!("unable to determine home directory"))?;
        let dev_dir = Utf8PathBuf::from_path_buf(home.join(".dev"))
            .map_err(|_| anyhow!("config path must be valid UTF-8"))?;

        let platform_path = dev_dir.join(format!("config.{}.toml", platform_config_suffix()));
        if platform_path.exists() {
            return Ok(ResolvedConfigPath {
                path: platform_path,
                source: ConfigPathSource::HomeDefault,
            });
        }

        let path = dev_dir.join("config.toml");
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
        let config_root = config_root_dir(&config_path);

        let requested_project = ctx
            .project
            .clone()
            .or_else(|| config.default_project.clone());
        let mut project_language: Option<String> = None;

        if let Some(project) = requested_project.as_deref() {
            let projects = config.projects.as_ref().with_context(|| {
                format!("project `{}` requested but no projects configured", project)
            })?;
            let spec = projects
                .get(project)
                .with_context(|| format!("unknown project `{}`", project))?;

            if let Some(chdir) = &spec.chdir {
                let chdir_path = Path::new(chdir);
                let target = if chdir_path.is_absolute() {
                    chdir_path.to_path_buf()
                } else {
                    config_root.join(chdir_path)
                };

                std::env::set_current_dir(&target).with_context(|| {
                    format!(
                        "changing directory to project `{}` at {}",
                        project,
                        target.display()
                    )
                })?;
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
        self.ctx.effective_language(
            &self.config,
            self.project_language.as_deref(),
            override_lang,
        )
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

fn handle_walk(ctx: &CliContext, request: WalkRequest) -> Result<()> {
    use crate::walk::{WalkOptions, generate_manifest};

    if ctx.dry_run {
        println!(
            "[dry-run] Generate manifest for {} -> {}",
            request.directory.display(),
            request.output.display()
        );
        return Ok(());
    }

    let opts = WalkOptions {
        max_depth: request.max_depth as usize,
        include_content: !request.no_content,
        extensions: request.extensions,
        ignore_hidden: !request.include_hidden,
    };

    println!("Generating directory manifest...");
    let manifest = generate_manifest(&request.directory, opts)?;

    std::fs::write(&request.output, manifest)?;

    println!(
        "Directory map generated successfully: {}",
        request.output.display()
    );

    Ok(())
}

fn handle_review(
    ctx: &CliContext,
    output: Option<PathBuf>,
    include_working: bool,
    main: bool,
) -> Result<()> {
    use crate::review::{ReviewOptions, generate_review, get_repo_root};

    if ctx.dry_run {
        let output_path = output
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "diff.md".to_string());
        println!("[dry-run] Generate review report -> {}", output_path);
        return Ok(());
    }

    let opts = ReviewOptions {
        include_working,
        compare_main: main,
    };

    let repo_root = get_repo_root()?;

    println!("Generating code review report...");
    let report = generate_review(opts, &repo_root)?;

    let output_path = output.unwrap_or_else(|| PathBuf::from("diff.md"));

    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    std::fs::write(&output_path, report)?;

    println!(
        "Review report generated successfully: {}",
        output_path.display()
    );

    Ok(())
}

fn handle_setup(
    ctx: &CliContext,
    command: Option<SetupCommand>,
    _root_skip_installed: bool,
    root_no_deps: bool,
) -> Result<()> {
    use crate::setup::{Component, SetupConfig, SetupContext};

    // Create log file path
    let home = dirs::home_dir().context("Could not determine home directory")?;
    let log_file = home.join(".dev").join("setup.log");

    // Ensure .dev directory exists
    if let Some(parent) = log_file.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Create setup context
    let setup_config = SetupConfig::default();
    let setup_ctx = SetupContext::new(ctx.dry_run, Some(log_file), setup_config)?;

    match command {
        None => {
            // Default: run default components with --skip-installed implied (unless overridden)
            let components: Result<Vec<Component>> = setup_ctx
                .config
                .default_components
                .iter()
                .map(|name| Component::from_str(name))
                .collect();

            let components = components?;
            crate::setup::run_setup(&setup_ctx, components, true, root_no_deps)?;
        }
        Some(SetupCommand::Run {
            components: component_names,
            skip_installed,
            no_deps,
        }) => {
            let components: Result<Vec<Component>> = component_names
                .iter()
                .map(|name| Component::from_str(name))
                .collect();

            let components = components?;
            // Subcommand flags take precedence over root flags
            crate::setup::run_setup(&setup_ctx, components, skip_installed, no_deps)?;
        }
        Some(SetupCommand::Inference {
            service,
            dest,
            force,
            no_cache,
        }) => {
            let home = dirs::home_dir().context("Could not determine home directory")?;
            let default_dest = home.join("repos").join("inference").join(service.trim());
            let dest = dest.unwrap_or(default_dest);

            let service = service.trim();
            if service.is_empty() {
                bail!("inference service name cannot be empty");
            }

            let repo = format!("dev-{}", service);
            let repo_url = format!("https://github.com/bakobiibizo/{}.git", repo);

            if ctx.dry_run {
                println!("[dry-run] clone/update {} -> {}", repo_url, dest.display());
                let script = dest.join("scripts").join("setup.sh");
                let mut argv = vec!["bash".to_owned(), script.display().to_string()];
                if no_cache {
                    argv.push("--no-cache".to_owned());
                }
                println!(
                    "[dry-run] run: {} (cwd: {})",
                    format_command(&argv),
                    dest.display()
                );
                return Ok(());
            }

            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent)
                    .with_context(|| format!("creating parent directory {}", parent.display()))?;
            }

            if dest.exists() {
                let git_dir = dest.join(".git");
                if git_dir.exists() {
                    let argv = vec![
                        "git".to_owned(),
                        "-C".to_owned(),
                        dest.display().to_string(),
                        "pull".to_owned(),
                        "--ff-only".to_owned(),
                    ];
                    println!("Updating inference repo: {}", format_command(&argv));
                    let status = run_process_streaming(&argv)?;
                    if !status.success() {
                        bail!(
                            "command `{}` failed with exit code {:?}",
                            format_command(&argv),
                            status.code()
                        );
                    }
                } else if force {
                    println!(
                        "[warn] removing existing destination {} (--force)",
                        dest.display()
                    );
                    std::fs::remove_dir_all(&dest)
                        .with_context(|| format!("removing {}", dest.display()))?;

                    let argv = vec![
                        "git".to_owned(),
                        "clone".to_owned(),
                        repo_url.clone(),
                        dest.display().to_string(),
                    ];
                    println!("Cloning inference repo: {}", format_command(&argv));
                    let status = run_process_streaming(&argv)?;
                    if !status.success() {
                        bail!(
                            "command `{}` failed with exit code {:?}",
                            format_command(&argv),
                            status.code()
                        );
                    }
                } else {
                    bail!(
                        "destination {} already exists; rerun with --force or pass --dest",
                        dest.display()
                    );
                }
            } else {
                let argv = vec![
                    "git".to_owned(),
                    "clone".to_owned(),
                    repo_url.clone(),
                    dest.display().to_string(),
                ];
                println!("Cloning inference repo: {}", format_command(&argv));
                let status = run_process_streaming(&argv)?;
                if !status.success() {
                    bail!(
                        "command `{}` failed with exit code {:?}",
                        format_command(&argv),
                        status.code()
                    );
                }
            }

            let script = dest.join("scripts").join("setup.sh");
            if !script.exists() {
                bail!(
                    "expected setup script at {} (repo contract: scripts/setup.sh)",
                    script.display()
                );
            }

            // Avoid cross-project container naming collisions: many repos hardcode `container_name:`.
            // Compose already namespaces names by project; we strip explicit container names.
            let compose_candidates = [
                dest.join("docker-compose.yml"),
                dest.join("docker-compose.yaml"),
                dest.join("compose.yml"),
                dest.join("compose.yaml"),
            ];
            for path in compose_candidates.iter() {
                if strip_compose_container_name(path)? {
                    println!("[warn] removed container_name from {}", path.display());
                }
            }

            let mut argv = vec!["bash".to_owned(), script.display().to_string()];
            if no_cache {
                argv.push("--no-cache".to_owned());
            }

            println!("Running inference setup: {}", format_command(&argv));
            let status = run_process_streaming_in_dir(&argv, &dest)?;
            if !status.success() {
                bail!(
                    "command `{}` failed with exit code {:?}",
                    format_command(&argv),
                    status.code()
                );
            }
        }
        Some(SetupCommand::All {
            skip_installed,
            no_deps,
        }) => {
            let components = Component::all();
            // Subcommand flags take precedence over root flags
            crate::setup::run_setup(&setup_ctx, components, skip_installed, no_deps)?;
        }
        Some(SetupCommand::Status) => {
            crate::setup::show_status(&setup_ctx)?;
        }
        Some(SetupCommand::List) => {
            crate::setup::list_components()?;
        }
        Some(SetupCommand::Config) => {
            println!("Setup Configuration");
            println!("===================\n");
            println!("Architecture: {}", setup_ctx.arch.as_str());
            println!("Platform: {}", setup_ctx.platform.as_str());
            println!("Package Manager: {}", setup_ctx.platform.package_manager());
            println!("Sudo Available: {}", setup_ctx.sudo);
            println!("\nDefault Components:");
            for component in &setup_ctx.config.default_components {
                println!("  - {}", component);
            }
            if !setup_ctx.config.skip_components.is_empty() {
                println!("\nSkip Components:");
                for component in &setup_ctx.config.skip_components {
                    println!("  - {}", component);
                }
            }
            println!("\nNode Version: {}", setup_ctx.config.node_version);
            if let Some(cuda_version) = &setup_ctx.config.cuda_version {
                println!("CUDA Version: {}", cuda_version);
            }
        }
    }

    Ok(())
}
