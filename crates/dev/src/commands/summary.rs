use std::io::Write;
use std::process::{Command as ProcessCommand, Stdio};

use anyhow::{Context, Result, bail};

use crate::cli::SummaryCommand;
use crate::config::DevConfig;
use crate::core::exec::{CapturedCommandResult, format_command, run_process_captured_in_shell};
use crate::core::output::{bounded_output, combine_output};
use crate::dispatch::AppState;
use crate::tasks::CommandSpec;

pub(crate) fn run_task_sequence_summarized(state: &AppState, tasks: &[String]) -> Result<()> {
    for task in tasks {
        println!("Running summarized task `{}`", task);
        let commands = state.tasks.flatten(task)?;
        execute_commands_summarized(state, task, &commands, false)?;
    }
    Ok(())
}

#[derive(Debug)]
struct SummaryOptions {
    shell: String,
    max_output_bytes: usize,
    tail_bytes: usize,
    llm_command: Option<String>,
}

pub(crate) fn handle(state: &AppState, command: SummaryCommand) -> Result<()> {
    match command {
        SummaryCommand::Run(args) => handle_run(state, &args.task, args.raw),
        SummaryCommand::Exec(args) => handle_exec(state, &args.argv, args.raw),
    }
}

pub(crate) fn handle_run(state: &AppState, task: &str, raw: bool) -> Result<()> {
    println!("Running summarized task `{}`", task);
    let commands = state.tasks.flatten(task)?;
    execute_commands_summarized(state, task, &commands, raw)
}

pub(crate) fn handle_exec(state: &AppState, argv: &[String], raw: bool) -> Result<()> {
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

pub(crate) fn execute_commands_summarized(
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
                "[warn] {} failed with exit code {} (ignored)",
                render,
                crate::dispatch::exit_code_display(result.status)
            );
        } else {
            eprintln!(
                "command `{}` failed with exit code {}",
                render,
                crate::dispatch::exit_code_display(result.status)
            );
            std::process::exit(result.status.code().unwrap_or(1));
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
         Exit code: {}\n\
         Duration: {:.2?}\n\n\
         Captured output:\n{output}\n",
        crate::dispatch::exit_code_display(result.status),
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
            "LLM summary command exited with {}: {}",
            crate::dispatch::exit_code_display(output.status),
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
        let mut tail = lines.by_ref().rev().take(12).collect::<Vec<_>>();
        tail.reverse();
        tail
    } else {
        interesting
    };

    let mut out = String::new();
    out.push_str("Summary\n");
    out.push_str(&format!("- command: {}\n", command));
    out.push_str(&format!(
        "- exit: {}\n",
        crate::dispatch::exit_code_display(result.status)
    ));
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

fn print_raw_output(stdout: &str, stderr: &str) {
    if !stdout.is_empty() {
        println!("Raw stdout\n{}", stdout);
    }
    if !stderr.is_empty() {
        println!("Raw stderr\n{}", stderr);
    }
}
