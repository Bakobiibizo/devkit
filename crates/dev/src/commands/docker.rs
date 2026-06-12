use std::process::{Command as ProcessCommand, Stdio};

use anyhow::{Context, Result, bail};

use crate::cli::{
    DockerBuildArgs, DockerCommand, DockerComposeCommand, DockerComposeUpBuildArgs,
    DockerComposeUpCommand, DockerInitArgs,
};
use crate::core::exec::{format_command, run_process};
use crate::dispatch::AppState;
use crate::{dockergen, envfile};

pub(crate) fn handle(state: &AppState, command: DockerCommand) -> Result<()> {
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
