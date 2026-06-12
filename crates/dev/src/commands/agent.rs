use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command as ProcessCommand, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, anyhow, bail};

use crate::cli::AgentCommand;
use crate::core::exec::{format_command, shell_command};
use crate::dispatch::{AppState, exit_code_display};

#[derive(Debug)]
struct AgentLaunch {
    argv: Vec<String>,
    cwd: PathBuf,
    prompt: String,
    model: Option<String>,
    prompt_stdin: bool,
    iterations: u32,
}

pub(crate) fn handle(state: &AppState, command: AgentCommand) -> Result<()> {
    match command {
        AgentCommand::Run(args) => handle_run(state, args),
        AgentCommand::List => handle_list(),
        AgentCommand::Status(args) => handle_status(&args.job_id, args.tail),
    }
}

pub(crate) fn handle_run(state: &AppState, args: crate::cli::AgentRunArgs) -> Result<()> {
    let agent_name = if args.agent == "default" {
        state
            .config
            .default_agent
            .clone()
            .unwrap_or_else(|| args.agent.clone())
    } else {
        args.agent.clone()
    };

    let agents = state.config.agents.as_ref().ok_or_else(|| {
        anyhow!(
            "no agents configured; add an [agents.{agent_name}] table to {}",
            state.config_path
        )
    })?;
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
    status_path: PathBuf,
    command: String,
    model: Option<String>,
    cwd: PathBuf,
    iterations: u32,
    started_at: u64,
}

pub(crate) fn handle_list() -> Result<()> {
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
        let state = agent_job_state(&job);
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

pub(crate) fn handle_status(job_id: &str, tail: usize) -> Result<()> {
    let path = agent_jobs_dir()?.join(format!("{}.job", job_id));
    let job = read_agent_job(&path).with_context(|| format!("reading job `{}`", job_id))?;
    println!("Job: {}", job.id);
    println!("Agent: {}", job.agent);
    println!("State: {}", agent_job_state(&job));
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
            "agent iteration {}/{} exited with code {}",
            iteration,
            launch.iterations,
            exit_code_display(status)
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
    let status_path = agent_status_path(&job_id)?;
    let stdout = fs::File::create(&log_path)
        .with_context(|| format!("creating agent log {}", log_path.display()))?;
    let stderr = stdout
        .try_clone()
        .with_context(|| format!("cloning agent log {}", log_path.display()))?;

    let script = detached_agent_script(&launch.argv, launch.iterations, launch.prompt_stdin);
    let mut command = ProcessCommand::new("bash");
    command.arg("-lc").arg(script);
    command.current_dir(&launch.cwd);
    command.env("DEV_AGENT_PROMPT", &launch.prompt);
    command.env("DEV_AGENT_CWD", &launch.cwd);
    command.env("DEV_AGENT_ITERATIONS", launch.iterations.to_string());
    command.env("DEV_AGENT_STATUS_PATH", &status_path);
    if let Some(model) = &launch.model {
        command.env("DEV_AGENT_MODEL", model);
    }
    command.stdin(Stdio::null());
    command.stdout(Stdio::from(stdout));
    command.stderr(Stdio::from(stderr));

    let child = command.spawn().with_context(|| {
        format!(
            "launching detached agent `{}`",
            format_command(&launch.argv)
        )
    })?;
    let record = AgentJobRecord {
        id: job_id,
        agent: agent_name.to_owned(),
        pid: child.id(),
        log_path: log_path.clone(),
        status_path: status_path.clone(),
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
    println!("Status: {}", status_path.display());
    println!("Check later: dev agent status {}", record.id);
    Ok(())
}

fn detached_agent_script(argv: &[String], iterations: u32, prompt_stdin: bool) -> String {
    let command = shell_command(argv);
    let display = shell_single_quote(&format_command(argv));
    let invoke = if prompt_stdin {
        format!("printf '%s' \"$DEV_AGENT_PROMPT\" | DEV_AGENT_ITERATION=\"$i\" {command}")
    } else {
        format!("DEV_AGENT_ITERATION=\"$i\" {command}")
    };
    format!(
        "status=0; for i in $(seq 1 {iterations}); do if [ {iterations} -gt 1 ]; then printf '[iteration %s/{iterations}] %s\\n' \"$i\" {display}; fi; {invoke}; status=$?; if [ $status -ne 0 ]; then break; fi; done; finished_at=$(date +%s); tmp=\"$DEV_AGENT_STATUS_PATH.tmp\"; {{ printf 'exit_code=%s\\n' \"$status\"; printf 'finished_at=%s\\n' \"$finished_at\"; }} > \"$tmp\" && mv \"$tmp\" \"$DEV_AGENT_STATUS_PATH\"; exit $status",
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

fn agent_status_path(job_id: &str) -> Result<PathBuf> {
    let home = dirs::home_dir().context("determining home directory")?;
    let dir = home.join(".dev").join("agents").join("status");
    fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
    Ok(dir.join(format!("{}.status", job_id)))
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
    out.push_str(&format!("status_path={}\n", record.status_path.display()));
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
    let id = get("id")?;
    let status_path = values
        .get("status_path")
        .map(PathBuf::from)
        .unwrap_or(agent_status_path(&id)?);
    Ok(AgentJobRecord {
        id,
        agent: get("agent")?,
        pid: get("pid")?.parse().context("parsing job pid")?,
        log_path: PathBuf::from(get("log_path")?),
        status_path,
        command: get("command")?.replace("\\n", "\n"),
        model: if model.is_empty() { None } else { Some(model) },
        cwd: PathBuf::from(get("cwd")?),
        iterations: get("iterations")?.parse().context("parsing iterations")?,
        started_at: get("started_at")?.parse().context("parsing started_at")?,
    })
}

fn agent_job_state(job: &AgentJobRecord) -> String {
    if process_is_running(job.pid) {
        return "running".to_owned();
    }

    match read_agent_exit_code(&job.status_path) {
        Ok(Some(0)) => "ok".to_owned(),
        Ok(Some(code)) => format!("failed({code})"),
        Ok(None) => "exited".to_owned(),
        Err(_) => "exited".to_owned(),
    }
}

fn read_agent_exit_code(path: &Path) -> Result<Option<i32>> {
    if !path.exists() {
        return Ok(None);
    }
    let raw = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    for line in raw.lines() {
        if let Some(value) = line.strip_prefix("exit_code=") {
            return Ok(value.parse().ok());
        }
    }
    Ok(None)
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
