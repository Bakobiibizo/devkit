use std::fs;

use anyhow::{Context, Result, anyhow, bail};

use crate::cli::{EnvArgs, EnvCommand};
use crate::dispatch::AppState;
use crate::envfile;

pub(crate) fn handle(state: &AppState, args: EnvArgs) -> Result<()> {
    match args.command {
        Some(EnvCommand::List) | None => env_list(state, args.raw),
        Some(EnvCommand::Get { key }) => env_get(state, &key),
        Some(EnvCommand::Add { key, value }) => env_add(state, &key, &value),
        Some(EnvCommand::Rm { key }) => env_remove(state, &key),
        Some(EnvCommand::Profiles) => env_profiles(state),
        Some(EnvCommand::Switch { profile }) => env_switch(state, &profile),
        Some(EnvCommand::Save { name }) => env_save(state, &name),
        Some(EnvCommand::Check) => env_check(state),
        Some(EnvCommand::Init) => env_init(state),
        Some(EnvCommand::Template) => env_template(state),
        Some(EnvCommand::Diff { reference }) => env_diff(state, &reference),
        Some(EnvCommand::Sync { reference }) => env_sync(state, &reference),
    }
}

fn env_list(state: &AppState, raw: bool) -> Result<()> {
    let env_path = state.env_path()?;
    let env = envfile::EnvFile::load(&env_path)?;
    let mut entries: Vec<_> = env.entries().collect();
    entries.sort_by(|a, b| a.0.cmp(b.0));

    if entries.is_empty() {
        println!("No environment variables defined in {}.", env.path());
        return Ok(());
    }

    println!("Environment variables in {}:", env.path());
    for (key, value) in entries {
        if raw {
            println!("  {}={}", key, value);
        } else {
            let mask = if value.is_empty() { "" } else { "*****" };
            println!("  {}={}", key, mask);
        }
    }
    Ok(())
}

fn env_get(state: &AppState, key: &str) -> Result<()> {
    let env_path = state.env_path()?;
    let env = envfile::EnvFile::load(&env_path)?;

    for (k, v) in env.entries() {
        if k == key {
            println!("{}", v);
            return Ok(());
        }
    }

    bail!("key `{}` not found in {}", key, env.path())
}

fn env_add(state: &AppState, key: &str, value: &str) -> Result<()> {
    let env_path = state.env_path()?;
    let mut env = envfile::EnvFile::load(&env_path)?;
    let existed = env.entries().any(|(existing, _)| existing == key);
    env.upsert(key, value);
    env.save()?;

    let target = env.path();
    if existed {
        println!("Updated {} in {}", key, target);
    } else {
        println!("Added {} to {}", key, target);
    }
    Ok(())
}

fn env_remove(state: &AppState, key: &str) -> Result<()> {
    let env_path = state.env_path()?;
    let mut env = envfile::EnvFile::load(&env_path)?;
    if env.remove(key) {
        env.save()?;
        println!("Removed {} from {}", key, env.path());
    } else {
        println!("Key {} not present in {}", key, env.path());
    }
    Ok(())
}

fn env_profiles(state: &AppState) -> Result<()> {
    let env_path = state.env_path()?;
    let dir = env_path
        .parent()
        .ok_or_else(|| anyhow!("cannot determine parent directory of {}", env_path))?;

    let mut profiles: Vec<String> = Vec::new();
    for entry in fs::read_dir(dir.as_std_path())? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with(".env.") && !name.ends_with(".example") {
            let profile = name.strip_prefix(".env.").unwrap_or(&name);
            profiles.push(profile.to_owned());
        }
    }

    if profiles.is_empty() {
        println!("No environment profiles found in {}.", dir);
        println!("Use `dev env save <name>` to create a profile.");
        return Ok(());
    }

    profiles.sort();
    println!("Available profiles in {}:", dir);
    for profile in profiles {
        println!("  - {}", profile);
    }
    Ok(())
}

fn env_switch(state: &AppState, profile: &str) -> Result<()> {
    let env_path = state.env_path()?;
    let dir = env_path
        .parent()
        .ok_or_else(|| anyhow!("cannot determine parent directory of {}", env_path))?;

    let profile_path = dir.join(format!(".env.{}", profile));
    if !profile_path.exists() {
        bail!(
            "profile `{}` not found at {}. Use `dev env profiles` to list available profiles.",
            profile,
            profile_path
        );
    }

    fs::copy(profile_path.as_std_path(), env_path.as_std_path())
        .with_context(|| format!("copying {} to {}", profile_path, env_path))?;

    println!(
        "Switched to profile `{}` (copied {} to {})",
        profile, profile_path, env_path
    );
    Ok(())
}

fn env_save(state: &AppState, name: &str) -> Result<()> {
    let env_path = state.env_path()?;
    if !env_path.exists() {
        bail!("no .env file found at {}", env_path);
    }

    let dir = env_path
        .parent()
        .ok_or_else(|| anyhow!("cannot determine parent directory of {}", env_path))?;

    let profile_path = dir.join(format!(".env.{}", name));
    fs::copy(env_path.as_std_path(), profile_path.as_std_path())
        .with_context(|| format!("copying {} to {}", env_path, profile_path))?;

    println!(
        "Saved current .env as profile `{}` at {}",
        name, profile_path
    );
    Ok(())
}

fn env_check(state: &AppState) -> Result<()> {
    let env_path = state.env_path()?;
    let env = envfile::EnvFile::load(&env_path)?;
    let entries: std::collections::HashSet<_> = env.entries().map(|(k, _)| k.to_owned()).collect();

    let required = state.config.env.as_ref().and_then(|e| e.required.as_ref());
    let optional = state.config.env.as_ref().and_then(|e| e.optional.as_ref());

    let mut missing_required: Vec<&str> = Vec::new();
    let mut empty_required: Vec<&str> = Vec::new();
    let mut missing_optional: Vec<&str> = Vec::new();

    if let Some(required) = required {
        for key in required {
            if !entries.contains(key.as_str()) {
                missing_required.push(key);
            } else {
                let value = env
                    .entries()
                    .find(|(k, _)| k == key)
                    .map(|(_, v)| v)
                    .unwrap_or("");
                if value.is_empty() {
                    empty_required.push(key);
                }
            }
        }
    }

    if let Some(optional) = optional {
        for key in optional {
            if !entries.contains(key.as_str()) {
                missing_optional.push(key);
            }
        }
    }

    println!("Checking {} against config requirements...", env_path);

    if missing_required.is_empty() && empty_required.is_empty() {
        println!("[ok] All required keys present and non-empty.");
    } else {
        if !missing_required.is_empty() {
            println!("[error] Missing required keys:");
            for key in &missing_required {
                println!("  - {}", key);
            }
        }
        if !empty_required.is_empty() {
            println!("[error] Empty required keys:");
            for key in &empty_required {
                println!("  - {}", key);
            }
        }
    }

    if !missing_optional.is_empty() {
        println!("[warn] Missing optional keys:");
        for key in &missing_optional {
            println!("  - {}", key);
        }
    }

    if !missing_required.is_empty() || !empty_required.is_empty() {
        bail!("environment validation failed");
    }

    Ok(())
}

fn env_init(state: &AppState) -> Result<()> {
    let env_path = state.env_path()?;
    if env_path.exists() {
        println!(".env already exists at {}. Nothing to do.", env_path);
        return Ok(());
    }

    let dir = env_path
        .parent()
        .ok_or_else(|| anyhow!("cannot determine parent directory of {}", env_path))?;

    let example_path = dir.join(".env.example");
    if !example_path.exists() {
        bail!(
            "no .env.example found at {}. Create one first or use `dev env template` to generate it.",
            example_path
        );
    }

    fs::copy(example_path.as_std_path(), env_path.as_std_path())
        .with_context(|| format!("copying {} to {}", example_path, env_path))?;

    println!("Initialized .env from {} at {}", example_path, env_path);
    Ok(())
}

fn env_template(state: &AppState) -> Result<()> {
    let env_path = state.env_path()?;
    let env = envfile::EnvFile::load(&env_path)?;

    let dir = env_path
        .parent()
        .ok_or_else(|| anyhow!("cannot determine parent directory of {}", env_path))?;

    let example_path = dir.join(".env.example");

    let mut output = String::new();
    output.push_str("# Environment template generated from .env\n");
    output.push_str("# Fill in the values for your environment\n\n");

    for (key, _) in env.entries() {
        output.push_str(&format!("{}=\n", key));
    }

    fs::write(example_path.as_std_path(), &output)
        .with_context(|| format!("writing {}", example_path))?;

    println!("Generated .env.example at {}", example_path);
    Ok(())
}

fn env_diff(state: &AppState, reference: &str) -> Result<()> {
    let env_path = state.env_path()?;
    let env = envfile::EnvFile::load(&env_path)?;
    let env_keys: std::collections::HashSet<_> = env.entries().map(|(k, _)| k.to_owned()).collect();

    let dir = env_path
        .parent()
        .ok_or_else(|| anyhow!("cannot determine parent directory of {}", env_path))?;

    let ref_path = dir.join(reference);
    if !ref_path.exists() {
        bail!("reference file not found at {}", ref_path);
    }

    let ref_env = envfile::EnvFile::load(&ref_path)?;
    let ref_keys: std::collections::HashSet<_> =
        ref_env.entries().map(|(k, _)| k.to_owned()).collect();

    let missing: Vec<_> = ref_keys.difference(&env_keys).collect();
    let extra: Vec<_> = env_keys.difference(&ref_keys).collect();

    println!("Comparing {} against {}:", env_path, ref_path);

    if missing.is_empty() && extra.is_empty() {
        println!("[ok] No differences found.");
        return Ok(());
    }

    if !missing.is_empty() {
        println!("Missing in .env (present in {}):", reference);
        for key in &missing {
            println!("  - {}", key);
        }
    }

    if !extra.is_empty() {
        println!("Extra in .env (not in {}):", reference);
        for key in &extra {
            println!("  + {}", key);
        }
    }

    Ok(())
}

fn env_sync(state: &AppState, reference: &str) -> Result<()> {
    let env_path = state.env_path()?;
    let mut env = envfile::EnvFile::load(&env_path)?;
    let env_keys: std::collections::HashSet<_> = env.entries().map(|(k, _)| k.to_owned()).collect();

    let dir = env_path
        .parent()
        .ok_or_else(|| anyhow!("cannot determine parent directory of {}", env_path))?;

    let ref_path = dir.join(reference);
    if !ref_path.exists() {
        bail!("reference file not found at {}", ref_path);
    }

    let ref_env = envfile::EnvFile::load(&ref_path)?;
    let ref_keys: std::collections::HashSet<_> =
        ref_env.entries().map(|(k, _)| k.to_owned()).collect();

    let missing: Vec<_> = ref_keys.difference(&env_keys).cloned().collect();

    if missing.is_empty() {
        println!(
            "No missing keys. {} is in sync with {}.",
            env_path, ref_path
        );
        return Ok(());
    }

    println!("Adding {} missing keys from {}:", missing.len(), reference);
    for key in &missing {
        let value = ref_env
            .entries()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v)
            .unwrap_or("");
        env.upsert(key, value);
        println!(
            "  + {}={}",
            key,
            if value.is_empty() { "(empty)" } else { "*****" }
        );
    }

    env.save()?;
    println!("Synced {} keys to {}", missing.len(), env_path);
    Ok(())
}
