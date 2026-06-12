use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};

use crate::cli::{ResearchCommand, ResearchInitArgs};
use crate::core::exec::{format_command, run_process_streaming_in_dir};
use crate::dispatch::AppState;

pub(crate) fn handle(state: &AppState, command: ResearchCommand) -> Result<()> {
    match command {
        ResearchCommand::Init(args) => research_init(state, args),
    }
}

fn research_init(state: &AppState, args: ResearchInitArgs) -> Result<()> {
    let target = if args.directory.is_absolute() {
        args.directory.clone()
    } else {
        std::env::current_dir()?.join(&args.directory)
    };
    let target = target.canonicalize().unwrap_or_else(|_| target.clone());

    let project_name = args.name.unwrap_or_else(|| {
        target
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| "research-project".to_owned())
    });
    let package_name = args
        .package
        .unwrap_or_else(|| normalize_package_name(&project_name));

    if target.exists() && target.read_dir()?.next().is_some() && !args.force {
        bail!(
            "target directory is not empty: {} (rerun with --force to overwrite scaffold files)",
            target.display()
        );
    }

    if state.ctx.dry_run {
        println!(
            "[dry-run] would initialize research project at {}",
            target.display()
        );
        println!("[dry-run] project name: {}", project_name);
        println!("[dry-run] package name: {}", package_name);
        if !args.skip_install {
            println!(
                "[dry-run] would run: uv add \"research-harness @ git+{}\"",
                args.harness_git
            );
        }
        return Ok(());
    }

    fs::create_dir_all(&target).with_context(|| format!("creating {}", target.display()))?;

    let project_yaml = format!(
        "name: {name}\nversion: \"0.1.0\"\ndescription: \"\"\n\nexperiments:\n  - id: example\n    module: experiments.example\n    callable: run\n    description: \"Example experiment\"\n\nconfig:\n  default: configs/default.yaml\n\ndatasets: []\n\nthresholds: configs/thresholds.yaml\n\noutputs:\n  format: parquet\n",
        name = project_name
    );

    let default_config = "# Default experiment configuration.\n";
    let thresholds = "thresholds: {}\n";
    let experiments_init = "\"\"\"Experiments package.\"\"\"\n";
    let example_exp = "\"\"\"Example experiment module.\"\"\"\n\n\ndef run(seed: int, output_dir, **kwargs):\n    \"\"\"Run the example experiment.\"\"\"\n    return {\"status\": \"ok\", \"seed\": seed}\n";
    let package_init = format!("\"\"\"Reusable package for {}.\"\"\"\n", project_name);
    let bindings_init = "\"\"\"Bindings for target systems.\"\"\"\n";
    let binding_example = "\"\"\"Example binding adapter for clean-room integrations.\"\"\"\n";
    let analysis_tpl = "# Analysis Report\n\n## Scope\n- Hypothesis:\n- Dataset(s):\n- Config + seed policy:\n\n## Results\n- Run IDs:\n- Threshold outcomes:\n- Key observations:\n\n## Risks / Caveats\n-\n";
    let synthesis_tpl = "# Meta-Synthesis\n\n## Experiments Included\n-\n\n## Cross-Experiment Findings\n-\n\n## Plain-Language Overview\n-\n";
    let env_example = "HARNESS_HOME=.harness\n";
    let env_local = "HARNESS_HOME=.harness\n";

    write_scaffold_file(&target.join("project.yaml"), &project_yaml, args.force)?;
    write_scaffold_file(
        &target.join("configs").join("default.yaml"),
        default_config,
        args.force,
    )?;
    write_scaffold_file(
        &target.join("configs").join("thresholds.yaml"),
        thresholds,
        args.force,
    )?;
    write_scaffold_file(
        &target.join("experiments").join("__init__.py"),
        experiments_init,
        args.force,
    )?;
    write_scaffold_file(
        &target.join("experiments").join("example.py"),
        example_exp,
        args.force,
    )?;
    write_scaffold_file(
        &target.join("src").join(&package_name).join("__init__.py"),
        &package_init,
        args.force,
    )?;
    write_scaffold_file(
        &target
            .join("src")
            .join(&package_name)
            .join("bindings")
            .join("__init__.py"),
        bindings_init,
        args.force,
    )?;
    write_scaffold_file(
        &target
            .join("src")
            .join(&package_name)
            .join("bindings")
            .join("example.py"),
        binding_example,
        args.force,
    )?;
    write_scaffold_file(
        &target.join("reports").join("templates").join("analysis.md"),
        analysis_tpl,
        args.force,
    )?;
    write_scaffold_file(
        &target
            .join("reports")
            .join("templates")
            .join("meta_synthesis.md"),
        synthesis_tpl,
        args.force,
    )?;
    write_scaffold_file(
        &target.join(".harness").join("runs").join(".gitkeep"),
        "",
        args.force,
    )?;
    write_scaffold_file(
        &target.join(".harness").join("datasets").join(".gitkeep"),
        "",
        args.force,
    )?;
    write_scaffold_file(&target.join(".env.example"), env_example, args.force)?;
    write_scaffold_file(&target.join(".env"), env_local, false)?;

    if !args.skip_install {
        let argv = vec![
            "uv".to_owned(),
            "add".to_owned(),
            format!("research-harness @ git+{}", args.harness_git),
        ];
        println!("Installing harness dependency: {}", format_command(&argv));
        let status = run_process_streaming_in_dir(&argv, &target)?;
        if !status.success() {
            bail!(
                "command `{}` failed with exit code {:?}",
                format_command(&argv),
                status.code()
            );
        }
    }

    println!("Research project initialized at {}", target.display());
    println!("  project.yaml");
    println!("  configs/default.yaml");
    println!("  configs/thresholds.yaml");
    println!("  experiments/example.py");
    println!("  src/{}/bindings/", package_name);
    println!("  reports/templates/");
    println!("  .harness/ (project-local run state)");
    println!("  .env with HARNESS_HOME=.harness");

    Ok(())
}

fn normalize_package_name(input: &str) -> String {
    let mut out = String::new();
    let mut prev_was_sep = false;
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            out.push(ch.to_ascii_lowercase());
            prev_was_sep = false;
        } else if !prev_was_sep {
            out.push('_');
            prev_was_sep = true;
        }
    }
    let trimmed = out.trim_matches('_').to_string();
    let mut final_name = if trimmed.is_empty() {
        "research_project".to_owned()
    } else {
        trimmed
    };
    if final_name
        .chars()
        .next()
        .map(|c| c.is_ascii_digit())
        .unwrap_or(false)
    {
        final_name = format!("pkg_{}", final_name);
    }
    final_name
}

fn write_scaffold_file(path: &Path, content: &str, force: bool) -> Result<()> {
    if path.exists() && !force {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }
    fs::write(path, content).with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}
