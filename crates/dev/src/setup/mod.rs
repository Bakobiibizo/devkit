mod component;
mod context;
mod cuda;
mod docker;
mod system;
mod templates;
mod tools;

pub use component::{Component, InstallState};
pub use context::{SetupConfig, SetupContext};

use anyhow::Result;

/// Main entry point for setup commands
pub fn run_setup(
    ctx: &SetupContext,
    components: Vec<Component>,
    skip_installed: bool,
    no_deps: bool,
) -> Result<()> {
    validate_components(&components)?;

    let ordered = if no_deps {
        components
    } else {
        resolve_dependencies(&components)?
    };

    for component in ordered {
        if skip_installed {
            let state = component.detect(ctx)?;
            if matches!(state, InstallState::Installed { .. }) {
                println!("[ok] {}: already installed; skipping", component.name());
                continue;
            }
        }

        component.install(ctx)?;
    }

    Ok(())
}

fn validate_components(components: &[Component]) -> Result<()> {
    if components.is_empty() {
        anyhow::bail!("No setup components specified");
    }

    let mut seen = std::collections::HashSet::new();
    for component in components {
        if !seen.insert(component) {
            anyhow::bail!("Duplicate component: {}", component.name());
        }
    }

    Ok(())
}

pub fn show_status(ctx: &SetupContext) -> Result<()> {
    let all_components = Component::all();

    println!("Setup Component Status");
    println!("======================");
    println!(
        "Platform: {} / {} (package manager: {})\n",
        ctx.platform.as_str(),
        ctx.arch.as_str(),
        ctx.platform.package_manager().unwrap_or("unsupported")
    );

    for component in all_components {
        let state = component.detect(ctx)?;
        match &state {
            InstallState::NotInstalled => {
                println!("{:26} [warn] not installed", component.name());
            }
            InstallState::Partial { reasons } => {
                println!(
                    "{:26} [warn] partial: {}",
                    component.name(),
                    reasons.join(", ")
                );
            }
            InstallState::Installed { version, .. } => {
                if let Some(v) = version {
                    println!("{:26} [ok] installed ({})", component.name(), v);
                } else {
                    println!("{:26} [ok] installed", component.name());
                }
            }
            InstallState::PresentButUnknown { reasons } => {
                println!(
                    "{:26} [warn] present but unknown: {}",
                    component.name(),
                    reasons.join(", ")
                );
            }
        }
    }

    Ok(())
}

pub fn list_components() -> Result<()> {
    let all_components = Component::all();

    println!("Available Setup Components");
    println!("==========================\n");

    for component in all_components {
        let deps = component.dependencies();
        let deps_str = if deps.is_empty() {
            "none".to_string()
        } else {
            deps.iter().map(|c| c.name()).collect::<Vec<_>>().join(", ")
        };

        println!(
            "{:26} deps: {:36} {}",
            component.name(),
            deps_str,
            component.description()
        );
    }

    Ok(())
}

fn resolve_dependencies(components: &[Component]) -> Result<Vec<Component>> {
    let mut result = Vec::new();
    let mut visited = std::collections::HashSet::new();
    let mut visiting = std::collections::HashSet::new();

    for component in components {
        visit(*component, &mut result, &mut visited, &mut visiting)?;
    }

    Ok(result)
}

fn visit(
    component: Component,
    result: &mut Vec<Component>,
    visited: &mut std::collections::HashSet<Component>,
    visiting: &mut std::collections::HashSet<Component>,
) -> Result<()> {
    if visited.contains(&component) {
        return Ok(());
    }

    if visiting.contains(&component) {
        anyhow::bail!(
            "Circular dependency detected involving {}",
            component.name()
        );
    }

    visiting.insert(component);

    for dep in component.dependencies() {
        visit(*dep, result, visited, visiting)?;
    }

    visiting.remove(&component);
    visited.insert(component);
    result.push(component);

    Ok(())
}
