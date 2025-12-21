use super::component::InstallState;
use super::context::SetupContext;
use anyhow::Result;

/// Detect NVIDIA container runtime
pub fn detect_nvidia_container_runtime(ctx: &SetupContext) -> Result<InstallState> {
    // Check if nvidia-container-cli exists
    if ctx.command_exists("nvidia-container-cli") {
        // Check docker info for nvidia runtime
        let output = std::process::Command::new("docker")
            .arg("info")
            .output();

        if let Ok(output) = output {
            if output.status.success() {
                let info = String::from_utf8_lossy(&output.stdout);
                if info.contains("nvidia") {
                    return Ok(InstallState::Installed {
                        version: None,
                        details: vec!["nvidia runtime configured in docker".to_string()],
                    });
                } else {
                    return Ok(InstallState::Partial {
                        reasons: vec!["nvidia-container-cli present but runtime not configured".to_string()],
                    });
                }
            }
        }

        Ok(InstallState::Partial {
            reasons: vec!["nvidia-container-cli present but docker not accessible".to_string()],
        })
    } else {
        Ok(InstallState::NotInstalled)
    }
}

/// Install NVIDIA container runtime
pub fn install_nvidia_container_runtime(ctx: &SetupContext) -> Result<()> {
    let component = "nvidia_container_runtime";

    if !ctx.command_exists("docker") {
        anyhow::bail!("Docker is required but not installed");
    }

    ctx.log.ok(component, "Adding NVIDIA container toolkit repository");

    // Download GPG key
    if !ctx.dry_run {
        let output = std::process::Command::new("sh")
            .arg("-c")
            .arg("curl -fsSL https://nvidia.github.io/libnvidia-container/gpgkey | sudo gpg --dearmor -o /usr/share/keyrings/nvidia-container-toolkit-keyring.gpg")
            .output()?;

        if !output.status.success() {
            anyhow::bail!("Failed to download NVIDIA GPG key");
        }
    } else {
        ctx.log.dry_run(component, "curl -fsSL https://nvidia.github.io/libnvidia-container/gpgkey | sudo gpg --dearmor -o /usr/share/keyrings/nvidia-container-toolkit-keyring.gpg");
    }

    // Add repository
    if !ctx.dry_run {
        let output = std::process::Command::new("sh")
            .arg("-c")
            .arg("curl -s -L https://nvidia.github.io/libnvidia-container/stable/deb/nvidia-container-toolkit.list | sed 's#deb https://#deb [signed-by=/usr/share/keyrings/nvidia-container-toolkit-keyring.gpg] https://#g' | sudo tee /etc/apt/sources.list.d/nvidia-container-toolkit.list")
            .output()?;

        if !output.status.success() {
            anyhow::bail!("Failed to add NVIDIA container toolkit repository");
        }
    } else {
        ctx.log.dry_run(component, "Add NVIDIA container toolkit repository");
    }

    ctx.execute(
        component,
        std::process::Command::new("sudo")
            .arg("apt-get")
            .arg("update"),
    )?;

    ctx.log.ok(component, "Installing NVIDIA container toolkit");

    ctx.execute(
        component,
        std::process::Command::new("sudo")
            .arg("apt-get")
            .arg("install")
            .arg("-y")
            .arg("nvidia-container-toolkit"),
    )?;

    ctx.log.ok(component, "Configuring Docker runtime");

    ctx.execute(
        component,
        std::process::Command::new("sudo")
            .arg("nvidia-ctk")
            .arg("runtime")
            .arg("configure")
            .arg("--runtime=docker"),
    )?;

    ctx.log.ok(component, "Restarting Docker");

    ctx.execute(
        component,
        std::process::Command::new("sudo")
            .arg("systemctl")
            .arg("restart")
            .arg("docker"),
    )?;

    ctx.log.ok(component, "NVIDIA container runtime installed successfully");

    Ok(())
}

/// Detect CUDA toolkit on host
pub fn detect_cuda_toolkit_host(ctx: &SetupContext) -> Result<InstallState> {
    // Check for nvidia-smi
    if ctx.command_exists("nvidia-smi") {
        let output = std::process::Command::new("nvidia-smi")
            .arg("--query-gpu=driver_version,compute_cap")
            .arg("--format=csv,noheader")
            .output();

        if let Ok(output) = output {
            if output.status.success() {
                let info = String::from_utf8_lossy(&output.stdout);
                let parts: Vec<&str> = info.trim().split(',').collect();
                
                if parts.len() >= 2 {
                    let driver = parts[0].trim();
                    let sm = parts[1].trim();

                    // Check for CUDA installation
                    let cuda_path = std::path::Path::new("/usr/local/cuda");
                    if cuda_path.exists() {
                        // Try to get CUDA version
                        let nvcc_output = std::process::Command::new("nvcc")
                            .arg("--version")
                            .output();

                        if let Ok(nvcc_output) = nvcc_output {
                            if nvcc_output.status.success() {
                                let version_text = String::from_utf8_lossy(&nvcc_output.stdout);
                                // Parse version from output like "release 12.0, V12.0.140"
                                let version = version_text
                                    .lines()
                                    .find(|line| line.contains("release"))
                                    .and_then(|line| {
                                        line.split("release")
                                            .nth(1)?
                                            .split(',')
                                            .next()?
                                            .trim()
                                            .to_string()
                                            .into()
                                    });

                                return Ok(InstallState::Installed {
                                    version,
                                    details: vec![
                                        format!("driver: {}", driver),
                                        format!("compute capability: {}", sm),
                                    ],
                                });
                            }
                        }

                        // CUDA directory exists but nvcc not working
                        return Ok(InstallState::PresentButUnknown {
                            reasons: vec![
                                "CUDA directory exists but version not parseable".to_string(),
                                format!("driver: {}", driver),
                                format!("compute capability: {}", sm),
                            ],
                        });
                    } else {
                        // nvidia-smi works but no CUDA toolkit
                        return Ok(InstallState::Partial {
                            reasons: vec![
                                "NVIDIA driver present but CUDA toolkit not installed".to_string(),
                                format!("driver: {}", driver),
                                format!("compute capability: {}", sm),
                            ],
                        });
                    }
                }
            }
        }

        // nvidia-smi exists but query failed - might be OEM setup
        Ok(InstallState::PresentButUnknown {
            reasons: vec!["nvidia-smi present but unable to query GPU info".to_string()],
        })
    } else {
        Ok(InstallState::NotInstalled)
    }
}

/// Install CUDA toolkit on host (validate-first by default)
pub fn install_cuda_toolkit_host(ctx: &SetupContext) -> Result<()> {
    let component = "cuda_toolkit_host";

    // First, detect current state
    let state = detect_cuda_toolkit_host(ctx)?;

    match state {
        InstallState::Installed { version, details } => {
            ctx.log.ok(component, &format!("CUDA toolkit already installed: {:?}", version));
            for detail in details {
                ctx.log.ok(component, &detail);
            }
            return Ok(());
        }
        InstallState::PresentButUnknown { reasons } => {
            ctx.log.warn(component, "CUDA installation detected but layout is non-standard (possibly OEM-provisioned)");
            for reason in reasons {
                ctx.log.warn(component, &reason);
            }
            ctx.log.warn(component, "Refusing to auto-install to protect existing setup");
            ctx.log.warn(component, "Use --install-cuda-toolkit with explicit confirmation if you want to proceed");
            return Ok(());
        }
        InstallState::Partial { reasons } => {
            ctx.log.warn(component, "Partial CUDA installation detected:");
            for reason in reasons {
                ctx.log.warn(component, &reason);
            }
            ctx.log.warn(component, "This component validates only by default");
            ctx.log.warn(component, "Use --install-cuda-toolkit to actually install CUDA toolkit");
            return Ok(());
        }
        InstallState::NotInstalled => {
            ctx.log.warn(component, "No CUDA installation detected");
            ctx.log.warn(component, "This component validates only by default");
            ctx.log.warn(component, "Use --install-cuda-toolkit to actually install CUDA toolkit");
            return Ok(());
        }
    }
}
