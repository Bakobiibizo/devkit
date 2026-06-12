use std::path::PathBuf;

use anyhow::Result;

use crate::dispatch::CliContext;

pub(crate) fn handle(
    ctx: &CliContext,
    output: Option<PathBuf>,
    include_working: bool,
    main: bool,
) -> Result<()> {
    use crate::review::{ReviewOptions, generate_review, get_repo_root};

    if ctx.dry_run {
        let output_path = output
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "diff.md".to_string());
        println!("[dry-run] Generate review report -> {}", output_path);
        return Ok(());
    }

    let opts = ReviewOptions {
        include_working,
        compare_main: main,
    };

    let repo_root = get_repo_root()?;

    println!("Generating code review report...");
    let report = generate_review(opts, &repo_root)?;

    let output_path = output.unwrap_or_else(|| PathBuf::from("diff.md"));

    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    std::fs::write(&output_path, report)?;

    println!(
        "Review report generated successfully: {}",
        output_path.display()
    );

    Ok(())
}
