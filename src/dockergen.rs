use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};

use crate::cli::DockerInitArgs;

pub fn init(args: &DockerInitArgs, dry_run: bool) -> Result<()> {
    let docker_dir = Path::new("docker");
    let dockerfile_path = docker_dir.join("Dockerfile.core");
    let compose_path = Path::new("docker-compose.yml");
    let env_path = Path::new(".env");

    let dockerfile = render_dockerfile_core(&args.base_image);
    let compose = render_compose(&args.service);
    let env_file = render_env(&args.core_image);

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

fn render_dockerfile_core(base_image: &str) -> String {
    format!(
        "FROM {base_image}\n\nENV DEBIAN_FRONTEND=noninteractive\nRUN apt-get update && apt-get install -y --no-install-recommends \\\n    git git-lfs curl ca-certificates \\\n    build-essential pkg-config \\\n    python3-venv python3-dev \\\n    && rm -rf /var/lib/apt/lists/*\n\nRUN pip install --no-cache-dir uv\n\nWORKDIR /workspace\n",
        base_image = base_image
    )
}

fn render_compose(service: &str) -> String {
    format!(
        "services:\n  {service}:\n    build:\n      context: .\n      dockerfile: docker/Dockerfile.core\n    image: ${{CORE_IMAGE:-devkit/core:cuda13}}\n    container_name: devkit-core\n    working_dir: /workspace\n    volumes:\n      - .:/workspace:cached\n      - devkit-cache:/root/.cache\n    environment:\n      - NVIDIA_VISIBLE_DEVICES=all\n      - NVIDIA_DRIVER_CAPABILITIES=compute,utility\n    deploy:\n      resources:\n        reservations:\n          devices:\n            - capabilities: [gpu]\n    user: \"${{UID:-1000}}:${{GID:-1000}}\"\n    command: [\"bash\", \"-l\"]\n\nvolumes:\n  devkit-cache:\n",
        service = service
    )
}

fn render_env(core_image: &str) -> String {
    format!("UID=1000\nGID=1000\nCORE_IMAGE={core_image}\n", core_image = core_image)
}
