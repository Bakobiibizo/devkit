mod component;
mod context;
mod system;
mod docker;
mod cuda;
mod tools;
mod templates;

pub use component::{Component, InstallState};
pub use context::{SetupContext, SetupConfig};

use anyhow::Result;

/// Main entry point for setup commands
pub fn run_setup(
    ctx: &SetupContext,
    components: Vec<Component>,
    skip_installed: bool,
    no_deps: bool,
) -> Result<()> {
    // Validate components
    validate_components(&components)?;

    // Resolve dependencies and topologically sort
    let ordered = if no_deps {
        components
    } else {
        resolve_dependencies(&components)?
    };

    // Run each component
    for component in ordered {
        if skip_installed {
            let state = component.detect(ctx)?;
            if matches!(state, InstallState::Installed { .. }) {
                println!("[skip] {} already installed", component.name());
                continue;
            }
        }

        component.install(ctx)?;
    }

    Ok(())
}

/// Validate components list
fn validate_components(components: &[Component]) -> Result<()> {
    if components.is_empty() {
        anyhow::bail!("No components specified");
    }

    // Check for duplicates
    let mut seen = std::collections::HashSet::new();
    for component in components {
        if !seen.insert(component) {
            anyhow::bail!("Duplicate component: {}", component.name());
        }
    }

    Ok(())
}

/// Show status of all components
pub fn show_status(ctx: &SetupContext) -> Result<()> {
    let all_components = Component::all();
    
    println!("Setup Component Status");
    println!("======================\n");

    for component in all_components {
        let state = component.detect(ctx)?;
        match &state {
            InstallState::NotInstalled => {
                println!("{:20} ❌ Not Installed", component.name());
            }
            InstallState::Partial { reasons } => {
                println!("{:20} ⚠️  Partial ({})", component.name(), reasons.join(", "));
            }
            InstallState::Installed { version, .. } => {
                if let Some(v) = version {
                    println!("{:20} ✓ Installed ({})", component.name(), v);
                } else {
                    println!("{:20} ✓ Installed", component.name());
                }
            }
            InstallState::PresentButUnknown { reasons } => {
                println!("{:20} ⚠️  Present but Unknown ({})", component.name(), reasons.join(", "));
            }
        }
    }

    Ok(())
}

/// List all available components and their dependencies
pub fn list_components() -> Result<()> {
    let all_components = Component::all();
    
    println!("Available Setup Components");
    println!("==========================\n");

    for component in all_components {
        let deps = component.dependencies();
        let deps_str = if deps.is_empty() {
            "none".to_string()
        } else {
            deps.iter()
                .map(|c| c.name())
                .collect::<Vec<_>>()
                .join(", ")
        };

        println!("{:20} deps: {}", component.name(), deps_str);
    }

    Ok(())
}

/// Resolve dependencies and return topologically sorted list
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
        anyhow::bail!("Circular dependency detected involving {}", component.name());
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
