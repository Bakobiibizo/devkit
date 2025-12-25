use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};

use crate::cli::DockerInitArgs;
use crate::templates;

pub fn init(args: &DockerInitArgs, dry_run: bool) -> Result<()> {
    let docker_dir = Path::new("docker");
    let dockerfile_path = docker_dir.join("Dockerfile.core");
    let compose_path = Path::new("docker-compose.yml");
    let env_path = Path::new(".env");

    let dockerfile = render_dockerfile_core(&args.base_image)?;
    let compose = render_compose(&args.service)?;
    let env_file = render_env();

    if dry_run {
        println!("[dry-run] would create {}", dockerfile_path.display());
        println!("[dry-run] would create {}", compose_path.display());
        println!("[dry-run] would create {}", env_path.display());
        return Ok(());
    }

    if !docker_dir.exists() {
        fs::create_dir_all(docker_dir).with_context(|| format!("creating {}", docker_dir.display()))?;
    }

    write_file(&dockerfile_path, &dockerfile, args.force)?;
    write_file(compose_path, &compose, args.force)?;
    write_file(env_path, &env_file, args.force)?;

    println!("Docker scaffolding complete");
    Ok(())
}

fn write_file(path: &Path, content: &str, force: bool) -> Result<()> {
    if path.exists() && !force {
        bail!("{} already exists; rerun with --force to overwrite", path.display());
    }
    fs::write(path, content).with_context(|| format!("writing {}", path.display()))
}

fn load_template(path: &str) -> Result<String> {
    templates::get_string(path)
}

fn render_dockerfile_core(base_image: &str) -> Result<String> {
    let template = load_template("docker/Dockerfile.core")?;
    Ok(template.replace("{{base_image}}", base_image))
}

fn render_compose(service: &str) -> Result<String> {
    let template = load_template("services/docker-compose.yml")?;
    Ok(template.replace("{{service}}", service))
}

fn render_env() -> String {
    "UID=1000\nGID=1000\n".to_owned()
}
