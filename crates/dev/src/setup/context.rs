use anyhow::Result;
use std::path::PathBuf;

/// Architecture of the system
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Architecture {
    X86_64,
    Aarch64,
}

impl Architecture {
    pub fn detect() -> Result<Self> {
        match std::env::consts::ARCH {
            "x86_64" => Ok(Architecture::X86_64),
            "aarch64" => Ok(Architecture::Aarch64),
            arch => anyhow::bail!("Unsupported architecture: {}", arch),
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Architecture::X86_64 => "x86_64",
            Architecture::Aarch64 => "aarch64",
        }
    }
}

/// Platform/OS of the system
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    Ubuntu,
    Debian,
    Unknown,
}

impl Platform {
    pub fn detect() -> Result<Self> {
        if let Ok(content) = std::fs::read_to_string("/etc/os-release") {
            return Ok(Self::from_os_release(&content));
        }

        Ok(Platform::Unknown)
    }

    fn from_os_release(content: &str) -> Self {
        let id = content.lines().find_map(|line| {
            let (key, value) = line.split_once('=')?;
            (key == "ID").then(|| value.trim_matches(['\"', '\'']).to_ascii_lowercase())
        });

        match id.as_deref() {
            Some("ubuntu") => Platform::Ubuntu,
            Some("debian") => Platform::Debian,
            _ => Platform::Unknown,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Platform::Ubuntu => "ubuntu",
            Platform::Debian => "debian",
            Platform::Unknown => "unknown",
        }
    }

    pub fn package_manager(&self) -> Option<&'static str> {
        match self {
            Platform::Ubuntu | Platform::Debian => Some("apt"),
            Platform::Unknown => None,
        }
    }
}

/// Setup logger for structured output
#[derive(Debug, Clone)]
pub struct SetupLogger {
    log_file: Option<PathBuf>,
}

impl SetupLogger {
    pub fn new(log_file: Option<PathBuf>) -> Self {
        Self { log_file }
    }

    pub fn ok(&self, component: &str, message: &str) {
        println!("[ok] {}: {}", component, message);
        self.log_to_file(component, "ok", message, None, None);
    }

    pub fn warn(&self, component: &str, message: &str) {
        println!("[warn] {}: {}", component, message);
        self.log_to_file(component, "warn", message, None, None);
    }

    pub fn error(&self, component: &str, message: &str) {
        eprintln!("[error] {}: {}", component, message);
        self.log_to_file(component, "error", message, None, None);
    }

    pub fn dry_run(&self, component: &str, command: &str) {
        println!("[dry-run] {}: {}", component, command);
    }

    pub fn log_command(
        &self,
        component: &str,
        command: &str,
        stdout: Option<&str>,
        stderr: Option<&str>,
        status: &str,
    ) {
        self.log_to_file(component, status, command, stdout, stderr);
    }

    fn log_to_file(
        &self,
        component: &str,
        status: &str,
        message: &str,
        stdout: Option<&str>,
        stderr: Option<&str>,
    ) {
        if let Some(ref path) = self.log_file {
            use std::io::Write;

            let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
            let mut content = format!(
                "\n== component: {} ==\ntime: {}\nstatus: {}\nmessage: {}\n",
                component, timestamp, status, message
            );

            if let Some(out) = stdout {
                content.push_str(&format!("stdout:\n{}\n", out));
            }

            if let Some(err) = stderr {
                content.push_str(&format!("stderr:\n{}\n", err));
            }

            if let Ok(mut file) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
            {
                let _ = file.write_all(content.as_bytes());
            }
        }
    }
}

/// Configuration for setup operations
#[derive(Debug, Clone)]
pub struct SetupConfig {
    pub cuda_version: Option<String>,
    pub nvidia_driver_version: Option<String>,
    pub cuda_driver_version: Option<String>,
    pub node_version: String,
    pub default_components: Vec<String>,
    pub skip_components: Vec<String>,
}

impl Default for SetupConfig {
    fn default() -> Self {
        Self {
            cuda_version: None,
            nvidia_driver_version: None,
            cuda_driver_version: None,
            node_version: "22".to_string(),
            default_components: vec![
                "system_packages".to_string(),
                "git_lfs".to_string(),
                "uv".to_string(),
                "rustup".to_string(),
                "node".to_string(),
                "pnpm".to_string(),
            ],
            skip_components: Vec::new(),
        }
    }
}

impl SetupConfig {
    /// Validate configuration
    pub fn validate(&self) -> Result<()> {
        fn reject_duplicates(values: &[String], field: &str) -> Result<()> {
            let mut seen = std::collections::HashSet::new();
            for value in values {
                if !seen.insert(value) {
                    anyhow::bail!("Duplicate component '{}' in {}", value, field);
                }
            }
            Ok(())
        }

        reject_duplicates(&self.default_components, "default_components")?;
        reject_duplicates(&self.skip_components, "skip_components")?;

        // Validate default_components
        for component_name in &self.default_components {
            use crate::setup::Component;
            Component::from_str(component_name).map_err(|_| {
                anyhow::anyhow!(
                    "Unknown component in default_components: {}",
                    component_name
                )
            })?;
        }

        // Validate skip_components
        for component_name in &self.skip_components {
            use crate::setup::Component;
            Component::from_str(component_name).map_err(|_| {
                anyhow::anyhow!("Unknown component in skip_components: {}", component_name)
            })?;
        }

        // Validate node_version is not empty
        if self.node_version.is_empty()
            || !self
                .node_version
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || ".-_".contains(character))
        {
            anyhow::bail!("node_version must contain only ASCII letters, numbers, '.', '-' or '_'");
        }

        // Check for conflicts between default and skip
        for component in &self.default_components {
            if self.skip_components.contains(component) {
                anyhow::bail!(
                    "Component '{}' appears in both default_components and skip_components",
                    component
                );
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{Platform, SetupConfig};

    #[test]
    fn platform_detection_uses_exact_id() {
        assert_eq!(
            Platform::from_os_release("NAME=Ubuntu\nID=ubuntu\n"),
            Platform::Ubuntu
        );
        assert_eq!(
            Platform::from_os_release("NAME=Not Ubuntu\nID=fedora\nID_LIKE=debian\n"),
            Platform::Unknown
        );
        assert_eq!(Platform::Unknown.package_manager(), None);
    }

    #[test]
    fn setup_config_rejects_duplicates_and_unsafe_versions() {
        let mut config = SetupConfig {
            default_components: vec!["uv".into(), "uv".into()],
            ..SetupConfig::default()
        };
        assert!(
            config
                .validate()
                .unwrap_err()
                .to_string()
                .contains("Duplicate")
        );

        config.default_components = vec!["uv".into()];
        config.node_version = "22; touch /tmp/bad".into();
        assert!(
            config
                .validate()
                .unwrap_err()
                .to_string()
                .contains("node_version")
        );
    }
}

/// Context passed to all setup components
#[derive(Debug, Clone)]
pub struct SetupContext {
    pub arch: Architecture,
    pub platform: Platform,
    pub dry_run: bool,
    pub sudo: bool,
    pub log: SetupLogger,
    pub config: SetupConfig,
}

impl SetupContext {
    pub fn new(dry_run: bool, log_file: Option<PathBuf>, config: SetupConfig) -> Result<Self> {
        let arch = Architecture::detect()?;
        let platform = Platform::detect()?;
        let log = SetupLogger::new(log_file);

        // Validate config
        config.validate()?;

        Ok(Self {
            arch,
            platform,
            dry_run,
            sudo: Self::check_sudo(),
            log,
            config,
        })
    }

    fn check_sudo() -> bool {
        // Check if we can run sudo commands
        std::process::Command::new("sudo")
            .arg("-n")
            .arg("true")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    /// Execute a command with proper logging
    pub fn execute(&self, component: &str, cmd: &mut std::process::Command) -> Result<()> {
        let cmd_str = format!("{:?}", cmd);

        if self.dry_run {
            self.log.dry_run(component, &cmd_str);
            return Ok(());
        }

        let output = cmd.output()?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        let status = if output.status.success() {
            "ok"
        } else {
            "error"
        };

        self.log
            .log_command(component, &cmd_str, Some(&stdout), Some(&stderr), status);

        if !output.status.success() {
            self.log.error(
                component,
                &format!("Command failed with status {}", output.status),
            );
            anyhow::bail!("Command failed with status {}: {}", output.status, stderr);
        }

        Ok(())
    }

    /// Check if a binary exists in PATH
    pub fn command_exists(&self, cmd: &str) -> bool {
        std::process::Command::new("which")
            .arg(cmd)
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }
}
