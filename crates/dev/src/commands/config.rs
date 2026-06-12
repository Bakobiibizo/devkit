use std::fs;
use std::io;

use anyhow::{Context, Result, anyhow, bail};
use camino::Utf8PathBuf;

use crate::cli::ConfigCommand;
use crate::config::{self, TaskUpdateMode};
use crate::core::exec::format_command;
use crate::dispatch::CliContext;
use crate::tasks::TaskIndex;

pub(crate) fn handle(ctx: &CliContext, command: Option<ConfigCommand>) -> Result<()> {
    let resolved = ctx.resolve_config_path()?;
    let config_path = resolved.path;
    match command {
        Some(ConfigCommand::Path) => {
            println!(
                "Config path: {} ({})",
                config_path,
                resolved.source.as_str()
            );
            Ok(())
        }
        None | Some(ConfigCommand::Show) => {
            if !config_path.exists() {
                println!("No config found at {}.", config_path);
                println!("Use `dev config generate` to scaffold a default configuration.");
                return Ok(());
            }

            let config = config::load_from_path(&config_path)?;
            println!(
                "Config path: {} ({})",
                config_path,
                resolved.source.as_str()
            );
            println!("{}", config::format_summary(&config));
            Ok(())
        }
        Some(ConfigCommand::Check) => {
            let config = config::load_from_path(&config_path)?;
            let _ = TaskIndex::from_config(&config)?;
            println!("Config OK: {} ({})", config_path, resolved.source.as_str());
            println!("{}", config::format_summary(&config));
            Ok(())
        }
        Some(ConfigCommand::Generate { path, force }) => {
            let target = match path {
                Some(path) => Utf8PathBuf::from_path_buf(path)
                    .map_err(|_| anyhow!("config generate path must be valid UTF-8"))?,
                None => config_path.clone(),
            };
            config::write_example_config(&target, force)?;
            if force {
                println!("Overwrote config at {}", target);
            } else {
                println!("Wrote example config to {}", target);
            }
            Ok(())
        }
        Some(ConfigCommand::Reload) => {
            if !config_path.exists() {
                println!("No config found at {}. Nothing to reload.", config_path);
                return Ok(());
            }
            let config = config::load_from_path(&config_path)?;
            println!(
                "Reloaded config from {} ({})",
                config_path,
                resolved.source.as_str()
            );
            println!("{}", config::format_summary(&config));
            Ok(())
        }
        Some(ConfigCommand::Add {
            name,
            command,
            force,
            append,
        }) => config_add(&config_path, name, command, force, append),
    }
}

fn config_add(
    config_path: &Utf8PathBuf,
    name: Option<String>,
    command: Vec<String>,
    force: bool,
    append: bool,
) -> Result<()> {
    let mut name = name;
    let mut command = command;

    if name.is_none() {
        name = Some(prompt("Task name: ")?);
    }
    let name = name
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow!("task name is required"))?;

    if command.is_empty() {
        let cmd = prompt("Command: ")?;
        if cmd.trim().is_empty() {
            bail!("command is required");
        }
        command = vec![cmd];
    }

    let (argv, render) = parse_config_add_command(&command)?;

    let existed = task_exists(config_path, &name)?;
    let mode = if append {
        TaskUpdateMode::Append
    } else if force {
        TaskUpdateMode::Overwrite
    } else if existed {
        let choice = prompt("Task exists. Overwrite, append, or cancel? (o/a/N): ")?;
        match choice.trim().to_lowercase().as_str() {
            "o" | "overwrite" => TaskUpdateMode::Overwrite,
            "a" | "append" => TaskUpdateMode::Append,
            _ => bail!("canceled"),
        }
    } else {
        TaskUpdateMode::Overwrite
    };

    println!("Config path: {}", config_path);
    println!("Task: {}", name);
    println!("Command: {}", render);
    if force || append || !existed {
        config::upsert_task_command(config_path, &name, &argv, mode)?;
        println!("Wrote task `{}` to {}", name, config_path);
        return Ok(());
    }

    let confirm = prompt("Write changes? (y/N): ")?;
    if confirm.trim().eq_ignore_ascii_case("y") {
        config::upsert_task_command(config_path, &name, &argv, mode)?;
        println!("Wrote task `{}` to {}", name, config_path);
        Ok(())
    } else {
        bail!("canceled")
    }
}

fn parse_config_add_command(command: &[String]) -> Result<(Vec<String>, String)> {
    if command.is_empty() {
        bail!("command is required");
    }

    if command.len() >= 2 && command[0] == "--" {
        let argv = command[1..].to_vec();
        if argv.is_empty() {
            bail!("argv after `--` must not be empty");
        }
        let render = format_command(&argv);
        return Ok((argv, render));
    }

    let cmd = command.join(" ");
    let argv = vec!["bash".to_owned(), "-lc".to_owned(), cmd.clone()];
    Ok((argv, format!("bash -lc {}", cmd)))
}

fn prompt(label: &str) -> Result<String> {
    print!("{}", label);
    io::Write::flush(&mut io::stdout()).with_context(|| format!("writing prompt `{label}`"))?;
    let mut buf = String::new();
    io::stdin()
        .read_line(&mut buf)
        .with_context(|| format!("reading input for `{label}`"))?;
    Ok(buf.trim_end_matches(['\n', '\r']).to_owned())
}

fn task_exists(path: &Utf8PathBuf, task_name: &str) -> Result<bool> {
    if !path.exists() {
        return Ok(false);
    }

    let raw = fs::read_to_string(path).with_context(|| format!("reading config {}", path))?;
    let doc: toml_edit::DocumentMut = raw
        .parse()
        .with_context(|| format!("parsing config {}", path))?;
    let Some(tasks) = doc.get("tasks").and_then(|item| item.as_table()) else {
        return Ok(false);
    };
    Ok(tasks.contains_key(task_name))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dispatch::{AppState, ConfigPathSource};
    use std::sync::{Mutex, OnceLock};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn cwd_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    fn unique_temp_dir() -> Utf8PathBuf {
        let mut dir = std::env::temp_dir();
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        dir.push(format!("devkit-test-{ts}"));
        Utf8PathBuf::from_path_buf(dir).unwrap()
    }

    #[test]
    fn resolve_config_prefers_nearest_discovered() {
        let _guard = cwd_lock().lock().unwrap();
        let root = unique_temp_dir();
        let nested = root.join("a").join("b");
        fs::create_dir_all(nested.as_std_path()).unwrap();
        fs::create_dir_all(root.join(".dev").as_std_path()).unwrap();
        let cfg = root.join(".dev").join("config.toml");
        fs::write(cfg.as_std_path(), "default_language = 'python'\n").unwrap();

        let old = std::env::current_dir().unwrap();
        std::env::set_current_dir(nested.as_std_path()).unwrap();

        let ctx = CliContext {
            chdir: None,
            file: None,
            project: None,
            language: None,
            dry_run: false,
            verbose: 0,
            no_color: false,
        };
        let resolved = ctx.resolve_config_path().unwrap();
        assert_eq!(resolved.source, ConfigPathSource::Discovered);
        assert!(resolved.path.ends_with(".dev/config.toml"));

        std::env::set_current_dir(old).unwrap();
        let _ = fs::remove_dir_all(root.as_std_path());
    }

    #[test]
    fn resolve_config_prefers_legacy_when_no_dotdev() {
        let _guard = cwd_lock().lock().unwrap();
        let root = unique_temp_dir();
        let nested = root.join("a").join("b");
        fs::create_dir_all(nested.as_std_path()).unwrap();
        fs::create_dir_all(root.join("tools").join("dev").as_std_path()).unwrap();
        let cfg = root.join("tools").join("dev").join("config.toml");
        fs::write(cfg.as_std_path(), "default_language = 'python'\n").unwrap();

        let old = std::env::current_dir().unwrap();
        std::env::set_current_dir(nested.as_std_path()).unwrap();

        let ctx = CliContext {
            chdir: None,
            file: None,
            project: None,
            language: None,
            dry_run: false,
            verbose: 0,
            no_color: false,
        };
        let resolved = ctx.resolve_config_path().unwrap();
        assert_eq!(resolved.source, ConfigPathSource::Discovered);
        assert!(resolved.path.ends_with("tools/dev/config.toml"));

        std::env::set_current_dir(old).unwrap();
        let _ = fs::remove_dir_all(root.as_std_path());
    }

    #[test]
    fn resolve_config_prefers_explicit_file() {
        let root = unique_temp_dir();
        fs::create_dir_all(root.as_std_path()).unwrap();
        let cfg = root.join("explicit.toml");
        fs::write(cfg.as_std_path(), "default_language = 'python'\n").unwrap();

        let ctx = CliContext {
            chdir: None,
            file: Some(cfg.as_std_path().to_path_buf()),
            project: None,
            language: None,
            dry_run: false,
            verbose: 0,
            no_color: false,
        };
        let resolved = ctx.resolve_config_path().unwrap();
        assert_eq!(resolved.source, ConfigPathSource::Explicit);
        assert!(resolved.path.ends_with("explicit.toml"));

        let _ = fs::remove_dir_all(root.as_std_path());
    }

    #[test]
    fn project_applies_chdir_and_language() {
        let _guard = cwd_lock().lock().unwrap();
        let root = unique_temp_dir();
        let proj_dir = root.join("web");
        fs::create_dir_all(proj_dir.as_std_path()).unwrap();
        fs::create_dir_all(root.join(".dev").as_std_path()).unwrap();
        let cfg = root.join(".dev").join("config.toml");
        fs::write(
            cfg.as_std_path(),
            r#"default_language = 'python'

[projects.web]
chdir = 'web'
language = 'typescript'
"#,
        )
        .unwrap();

        let old = std::env::current_dir().unwrap();
        std::env::set_current_dir(root.as_std_path()).unwrap();

        let ctx = CliContext {
            chdir: None,
            file: None,
            project: Some("web".to_owned()),
            language: None,
            dry_run: false,
            verbose: 0,
            no_color: false,
        };
        let state = AppState::new(ctx).unwrap();
        assert_eq!(
            std::env::current_dir().unwrap(),
            proj_dir.as_std_path().to_path_buf()
        );
        assert_eq!(
            state.effective_language(None).as_deref(),
            Some("typescript")
        );

        std::env::set_current_dir(old).unwrap();
        let _ = fs::remove_dir_all(root.as_std_path());
    }
}
