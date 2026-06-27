use std::io::Write;
use std::process::{Command as ProcessCommand, Stdio};

use anyhow::{Context, Result, bail};

use crate::config::DevConfig;
use crate::core::exec::{CapturedCommandResult, format_command, run_process_captured_in_shell};
use crate::core::output::{bounded_output, combine_output};
use crate::dispatch::AppState;
use crate::tasks::CommandSpec;

pub(crate) fn run_task_sequence_summarized(state: &AppState, tasks: &[String]) -> Result<()> {
    for task in tasks {
        println!("Running summarized task `{}`", task);
        let commands = state.tasks.flatten(task)?;
        execute_commands_summarized(state, task, &commands)?;
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

fn execute_commands_summarized(
    state: &AppState,
    task: &str,
    commands: &[CommandSpec],
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
    let failing_tests = extract_failing_tests(output);
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
                || lowered.starts_with("---- ")
                || lowered.trim() == "failures:"
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
    if !failing_tests.is_empty() {
        out.push_str("- failing tests:\n");
        for test in failing_tests.iter().take(12) {
            out.push_str("  - ");
            out.push_str(test);
            out.push('\n');
        }
    }
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

fn extract_failing_tests(output: &str) -> Vec<String> {
    let mut tests = Vec::new();
    let mut in_failures_list = false;
    for line in output.lines() {
        let trimmed = line.trim();
        if let Some(name) = trimmed
            .strip_prefix("test ")
            .and_then(|rest| rest.strip_suffix(" ... FAILED"))
        {
            push_unique(&mut tests, name.trim());
            continue;
        }
        if let Some(name) = trimmed
            .strip_prefix("---- ")
            .and_then(|rest| rest.strip_suffix(" stdout ----"))
        {
            push_unique(&mut tests, name.trim());
            continue;
        }
        if trimmed == "failures:" {
            in_failures_list = true;
            continue;
        }
        if in_failures_list {
            if trimmed.is_empty() {
                continue;
            }
            if trimmed.starts_with("test result:") || trimmed.starts_with("error:") {
                in_failures_list = false;
                continue;
            }
            if !trimmed.contains(' ') && !trimmed.contains(':') {
                push_unique(&mut tests, trimmed);
            }
        }
    }
    tests
}

fn push_unique(values: &mut Vec<String>, value: &str) {
    if !value.is_empty() && !values.iter().any(|existing| existing == value) {
        values.push(value.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::extract_failing_tests;

    #[test]
    fn extracts_rust_test_failures_from_cargo_output() {
        let output = "running 2 tests\ntest ok_test ... ok\ntest ldgr_code::filesystem_activity_keeps_silent_commands_alive ... FAILED\n\nfailures:\n\n---- ldgr_code::filesystem_activity_keeps_silent_commands_alive stdout ----\nthread panicked\n\nfailures:\n    ldgr_code::filesystem_activity_keeps_silent_commands_alive\n\ntest result: FAILED. 1 passed; 1 failed\n";
        assert_eq!(
            extract_failing_tests(output),
            vec!["ldgr_code::filesystem_activity_keeps_silent_commands_alive"]
        );
    }
}
