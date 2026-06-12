use std::time::Instant;

use anyhow::{Result, anyhow, bail};

use crate::cli::{InstallArgs, StartArgs, Verb};
use crate::config::DevConfig;
use crate::core::exec::{format_command, run_external_command, run_process};
use crate::dispatch::AppState;
use crate::tasks::CommandSpec;
use crate::{commands, scaffold};

pub(crate) fn handle_list(state: &AppState) -> Result<()> {
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

pub(crate) fn handle_run(state: &AppState, task: &str) -> Result<()> {
    println!("Running task `{}`", task);
    let commands = state.tasks.flatten(task)?;
    execute_commands(state, task, &commands)
}

pub(crate) fn handle_start(state: &AppState, args: StartArgs) -> Result<()> {
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

pub(crate) fn handle_verb(state: &AppState, verb: Verb) -> Result<()> {
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
    commands::summary::run_task_sequence_summarized(state, &tasks)
}

pub(crate) fn handle_all(state: &AppState, verb: Verb) -> Result<()> {
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
        commands::summary::run_task_sequence_summarized(state, &tasks)?;
    }

    if !any_ran {
        println!(
            "No languages define a `{}` pipeline; nothing to do.",
            verb.as_str()
        );
    }

    Ok(())
}

pub(crate) fn handle_install(state: &AppState, args: InstallArgs) -> Result<()> {
    let language = state.effective_language(args.language).ok_or_else(|| {
        anyhow!("no language selected; pass `dev install <language>` or configure default_language")
    })?;

    if !scaffold::is_supported_language(&language) {
        bail!("unsupported language scaffold: {language}");
    }

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
        preview_install_commands(&state.config, &language);
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

fn preview_install_commands(config: &DevConfig, language: &str) {
    match install_commands(config, language) {
        Some(commands) if !commands.is_empty() => {
            println!("[dry-run] provisioning commands for `{}`:", language);
            for command in commands {
                println!("  -> {}", format_command(&command));
            }
        }
        _ => println!(
            "[dry-run] no provisioning commands configured for `{}`",
            language
        ),
    }
}
