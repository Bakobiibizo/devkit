use std::path::Path;

use anyhow::{Context, Result, bail};

use crate::cli::SetupCommand;
use crate::core::exec::{format_command, run_process_streaming, run_process_streaming_in_dir};
use crate::dispatch::CliContext;

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

pub(crate) fn handle(
    ctx: &CliContext,
    command: Option<SetupCommand>,
    root_skip_installed: bool,
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
            let components = configured_components(
                &setup_ctx.config.default_components,
                &setup_ctx.config.skip_components,
            )?;
            crate::setup::run_setup(&setup_ctx, components, true, root_no_deps)?;
        }
        Some(SetupCommand::Run {
            components: component_names,
            skip_installed,
            no_deps,
        }) => {
            let components = parse_component_names(&component_names)?;
            crate::setup::run_setup(
                &setup_ctx,
                components,
                skip_installed || root_skip_installed,
                no_deps || root_no_deps,
            )?;
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
            let skip_components = setup_ctx
                .config
                .skip_components
                .iter()
                .map(String::as_str)
                .collect::<std::collections::HashSet<_>>();
            let components = Component::all()
                .into_iter()
                .filter(|component| !skip_components.contains(component.name()))
                .collect();
            crate::setup::run_setup(
                &setup_ctx,
                components,
                skip_installed || root_skip_installed,
                no_deps || root_no_deps,
            )?;
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
            println!(
                "Package Manager: {}",
                setup_ctx
                    .platform
                    .package_manager()
                    .unwrap_or("unsupported")
            );
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
            if let Some(driver_version) = &setup_ctx.config.nvidia_driver_version {
                println!("NVIDIA Driver Version: {}", driver_version);
            }
            if let Some(cuda_driver_version) = &setup_ctx.config.cuda_driver_version {
                println!("CUDA Driver Version: {}", cuda_driver_version);
            }
        }
    }

    Ok(())
}

fn parse_component_names(component_names: &[String]) -> Result<Vec<crate::setup::Component>> {
    component_names
        .iter()
        .map(|name| crate::setup::Component::from_str(name))
        .collect()
}

fn configured_components(
    default_components: &[String],
    skip_components: &[String],
) -> Result<Vec<crate::setup::Component>> {
    let skip_components = skip_components
        .iter()
        .map(String::as_str)
        .collect::<std::collections::HashSet<_>>();

    default_components
        .iter()
        .filter(|name| !skip_components.contains(name.as_str()))
        .map(|name| crate::setup::Component::from_str(name))
        .collect()
}
