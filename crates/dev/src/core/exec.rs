use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Command as ProcessCommand, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};

#[derive(Debug)]
pub(crate) struct CapturedCommandResult {
    pub(crate) status: ExitStatus,
    pub(crate) stdout: String,
    pub(crate) stderr: String,
    pub(crate) elapsed: Duration,
}

pub(crate) fn run_process_captured_in_shell(
    argv: &[String],
    shell: &str,
) -> Result<CapturedCommandResult> {
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

pub(crate) fn run_process(argv: &[String]) -> Result<ExitStatus> {
    let mut command = ProcessCommand::new(&argv[0]);
    if argv.len() > 1 {
        command.args(&argv[1..]);
    }
    command
        .status()
        .with_context(|| format!("executing `{}`", format_command(argv)))
}

pub(crate) fn format_command(argv: &[String]) -> String {
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

pub(crate) fn shell_command(argv: &[String]) -> String {
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

pub(crate) fn run_external_command(argv: &[String]) -> Result<()> {
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

pub(crate) fn run_process_streaming(argv: &[String]) -> Result<ExitStatus> {
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

pub(crate) fn run_process_streaming_in_dir(argv: &[String], cwd: &Path) -> Result<ExitStatus> {
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
