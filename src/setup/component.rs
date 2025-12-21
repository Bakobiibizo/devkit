use super::context::SetupContext;
use anyhow::Result;

/// Installation state of a component
#[derive(Debug, Clone, PartialEq)]
pub enum InstallState {
    NotInstalled,
    Partial { reasons: Vec<String> },
    Installed { version: Option<String>, details: Vec<String> },
    PresentButUnknown { reasons: Vec<String> },
}

/// Installation mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallMode {
    Apply,
    DryRun,
}

/// All available setup components
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Component {
    SystemPackages,
    GitLfs,
    Uv,
    Rustup,
    Node,
    Pnpm,
    Pm2,
    Docker,
    NvidiaContainerRuntime,
    CudaToolkitHost,
    Zoxide,
    Atuin,
    Ngrok,
    RmGuard,
}

impl Component {
    /// Get all available components
    pub fn all() -> Vec<Component> {
        vec![
            Component::SystemPackages,
            Component::GitLfs,
            Component::Uv,
            Component::Rustup,
            Component::Node,
            Component::Pnpm,
            Component::Pm2,
            Component::Docker,
            Component::NvidiaContainerRuntime,
            Component::CudaToolkitHost,
            Component::Zoxide,
            Component::Atuin,
            Component::Ngrok,
            Component::RmGuard,
        ]
    }

    /// Get component name
    pub fn name(&self) -> &'static str {
        match self {
            Component::SystemPackages => "system_packages",
            Component::GitLfs => "git_lfs",
            Component::Uv => "uv",
            Component::Rustup => "rustup",
            Component::Node => "node",
            Component::Pnpm => "pnpm",
            Component::Pm2 => "pm2",
            Component::Docker => "docker",
            Component::NvidiaContainerRuntime => "nvidia_container_runtime",
            Component::CudaToolkitHost => "cuda_toolkit_host",
            Component::Zoxide => "zoxide",
            Component::Atuin => "atuin",
            Component::Ngrok => "ngrok",
            Component::RmGuard => "rm_guard",
        }
    }

    /// Parse component from string
    pub fn from_str(s: &str) -> Result<Self> {
        match s {
            "system_packages" => Ok(Component::SystemPackages),
            "git_lfs" => Ok(Component::GitLfs),
            "uv" => Ok(Component::Uv),
            "rustup" => Ok(Component::Rustup),
            "node" => Ok(Component::Node),
            "pnpm" => Ok(Component::Pnpm),
            "pm2" => Ok(Component::Pm2),
            "docker" => Ok(Component::Docker),
            "nvidia_container_runtime" => Ok(Component::NvidiaContainerRuntime),
            "cuda_toolkit_host" => Ok(Component::CudaToolkitHost),
            "zoxide" => Ok(Component::Zoxide),
            "atuin" => Ok(Component::Atuin),
            "ngrok" => Ok(Component::Ngrok),
            "rm_guard" => Ok(Component::RmGuard),
            _ => anyhow::bail!("Unknown component: {}", s),
        }
    }

    /// Get component dependencies
    pub fn dependencies(&self) -> &'static [Component] {
        match self {
            Component::Pm2 => &[Component::Node, Component::Pnpm],
            Component::Docker => &[Component::SystemPackages],
            Component::NvidiaContainerRuntime => &[Component::Docker],
            Component::Pnpm => &[Component::Node],
            Component::GitLfs => &[Component::SystemPackages],
            _ => &[],
        }
    }

    /// Detect installation state (pure, side-effect free)
    pub fn detect(&self, ctx: &SetupContext) -> Result<InstallState> {
        match self {
            Component::SystemPackages => super::system::detect_system_packages(ctx),
            Component::GitLfs => super::system::detect_git_lfs(ctx),
            Component::Uv => super::system::detect_uv(ctx),
            Component::Rustup => super::system::detect_rustup(ctx),
            Component::Node => super::system::detect_node(ctx),
            Component::Pnpm => super::system::detect_pnpm(ctx),
            Component::Pm2 => super::system::detect_pm2(ctx),
            Component::Docker => super::docker::detect_docker(ctx),
            Component::NvidiaContainerRuntime => super::cuda::detect_nvidia_container_runtime(ctx),
            Component::CudaToolkitHost => super::cuda::detect_cuda_toolkit_host(ctx),
            Component::Zoxide => super::tools::detect_zoxide(ctx),
            Component::Atuin => super::tools::detect_atuin(ctx),
            Component::Ngrok => super::tools::detect_ngrok(ctx),
            Component::RmGuard => super::tools::detect_rm_guard(ctx),
        }
    }

    /// Install the component
    pub fn install(&self, ctx: &SetupContext) -> Result<()> {
        match self {
            Component::SystemPackages => super::system::install_system_packages(ctx),
            Component::GitLfs => super::system::install_git_lfs(ctx),
            Component::Uv => super::system::install_uv(ctx),
            Component::Rustup => super::system::install_rustup(ctx),
            Component::Node => super::system::install_node(ctx),
            Component::Pnpm => super::system::install_pnpm(ctx),
            Component::Pm2 => super::system::install_pm2(ctx),
            Component::Docker => super::docker::install_docker(ctx),
            Component::NvidiaContainerRuntime => super::cuda::install_nvidia_container_runtime(ctx),
            Component::CudaToolkitHost => super::cuda::install_cuda_toolkit_host(ctx),
            Component::Zoxide => super::tools::install_zoxide(ctx),
            Component::Atuin => super::tools::install_atuin(ctx),
            Component::Ngrok => super::tools::install_ngrok(ctx),
            Component::RmGuard => super::tools::install_rm_guard(ctx),
        }
    }
}
