use super::component::InstallState;
use super::context::SetupContext;
use anyhow::Result;

/// Detect Docker installation state
pub fn detect_docker(ctx: &SetupContext) -> Result<InstallState> {
    if ctx.command_exists("docker") {
        let output = std::process::Command::new("docker")
            .arg("--version")
            .output()?;

        if output.status.success() {
            let version = String::from_utf8_lossy(&output.stdout);
            let version_str = version.trim().to_string();

            // Check if docker service is running
            let service_output = std::process::Command::new("systemctl")
                .arg("is-active")
                .arg("docker")
                .output()?;

            let service_active = service_output.status.success();

            // Check if user is in docker group
            let groups_output = std::process::Command::new("groups")
                .output()?;
            
            let in_docker_group = if groups_output.status.success() {
                let groups = String::from_utf8_lossy(&groups_output.stdout);
                groups.contains("docker")
            } else {
                false
            };

            let mut details = vec![version_str.clone()];
            let mut reasons = Vec::new();

            if !service_active {
                reasons.push("docker service not active".to_string());
            } else {
                details.push("service active".to_string());
            }

            if !in_docker_group {
                reasons.push("user not in docker group".to_string());
            } else {
                details.push("user in docker group".to_string());
            }

            if reasons.is_empty() {
                Ok(InstallState::Installed {
                    version: Some(version_str),
                    details,
                })
            } else {
                Ok(InstallState::Partial { reasons })
            }
        } else {
            Ok(InstallState::NotInstalled)
        }
    } else {
        Ok(InstallState::NotInstalled)
    }
}

/// Install Docker
pub fn install_docker(ctx: &SetupContext) -> Result<()> {
    let component = "docker";

    ctx.log.ok(component, "Adding Docker repository");

    // Install prerequisites
    ctx.execute(
        component,
        std::process::Command::new("sudo")
            .arg("apt-get")
            .arg("install")
            .arg("-y")
            .arg("ca-certificates")
            .arg("curl"),
    )?;

    // Create keyrings directory
    ctx.execute(
        component,
        std::process::Command::new("sudo")
            .arg("install")
            .arg("-m")
            .arg("0755")
            .arg("-d")
            .arg("/etc/apt/keyrings"),
    )?;

    // Download Docker GPG key
    if !ctx.dry_run {
        let output = std::process::Command::new("sudo")
            .arg("curl")
            .arg("-fsSL")
            .arg("https://download.docker.com/linux/ubuntu/gpg")
            .arg("-o")
            .arg("/etc/apt/keyrings/docker.asc")
            .output()?;

        if !output.status.success() {
            anyhow::bail!("Failed to download Docker GPG key");
        }
    } else {
        ctx.log.dry_run(component, "sudo curl -fsSL https://download.docker.com/linux/ubuntu/gpg -o /etc/apt/keyrings/docker.asc");
    }

    ctx.execute(
        component,
        std::process::Command::new("sudo")
            .arg("chmod")
            .arg("a+r")
            .arg("/etc/apt/keyrings/docker.asc"),
    )?;

    // Add Docker repository
    let arch = match ctx.arch {
        super::context::Architecture::X86_64 => "amd64",
        super::context::Architecture::Aarch64 => "arm64",
    };

    let repo_line = format!(
        "deb [arch={} signed-by=/etc/apt/keyrings/docker.asc] https://download.docker.com/linux/ubuntu $(. /etc/os-release && echo \"$VERSION_CODENAME\") stable",
        arch
    );

    if !ctx.dry_run {
        let output = std::process::Command::new("sh")
            .arg("-c")
            .arg(format!("echo '{}' | sudo tee /etc/apt/sources.list.d/docker.list > /dev/null", repo_line))
            .output()?;

        if !output.status.success() {
            anyhow::bail!("Failed to add Docker repository");
        }
    } else {
        ctx.log.dry_run(component, &format!("echo '{}' | sudo tee /etc/apt/sources.list.d/docker.list", repo_line));
    }

    ctx.execute(
        component,
        std::process::Command::new("sudo")
            .arg("apt-get")
            .arg("update"),
    )?;

    ctx.log.ok(component, "Installing Docker");

    ctx.execute(
        component,
        std::process::Command::new("sudo")
            .arg("apt-get")
            .arg("install")
            .arg("-y")
            .arg("docker-ce")
            .arg("docker-ce-cli")
            .arg("containerd.io")
            .arg("docker-buildx-plugin")
            .arg("docker-compose-plugin"),
    )?;

    ctx.log.ok(component, "Configuring Docker permissions");

    // Create docker group (may already exist)
    let _ = std::process::Command::new("sudo")
        .arg("groupadd")
        .arg("-f")
        .arg("docker")
        .output();

    // Add user to docker group
    let user = std::env::var("USER").unwrap_or_else(|_| "root".to_string());
    ctx.execute(
        component,
        std::process::Command::new("sudo")
            .arg("usermod")
            .arg("-aG")
            .arg("docker")
            .arg(&user),
    )?;

    // Set permissions on docker directory
    let home = std::env::var("HOME")?;
    let docker_dir = format!("{}/.docker", home);
    
    if std::path::Path::new(&docker_dir).exists() {
        ctx.execute(
            component,
            std::process::Command::new("sudo")
                .arg("chown")
                .arg(format!("{}:{}", user, user))
                .arg("-R")
                .arg(&docker_dir),
        )?;

        ctx.execute(
            component,
            std::process::Command::new("sudo")
                .arg("chmod")
                .arg("g+rwx")
                .arg("-R")
                .arg(&docker_dir),
        )?;
    }

    ctx.log.ok(component, "Enabling Docker service");

    ctx.execute(
        component,
        std::process::Command::new("sudo")
            .arg("systemctl")
            .arg("enable")
            .arg("docker.service"),
    )?;

    ctx.execute(
        component,
        std::process::Command::new("sudo")
            .arg("systemctl")
            .arg("enable")
            .arg("containerd.service"),
    )?;

    ctx.execute(
        component,
        std::process::Command::new("sudo")
            .arg("systemctl")
            .arg("start")
            .arg("docker"),
    )?;

    ctx.log.ok(component, "Docker installed successfully");
    ctx.log.warn(component, "You may need to log out and back in for group membership to take effect");

    Ok(())
}
