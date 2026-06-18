use std::path::PathBuf;

use anyhow::Result;

use crate::dispatch::CliContext;

pub(crate) struct WalkRequest {
    pub(crate) directory: PathBuf,
    pub(crate) output: PathBuf,
    pub(crate) stdout: bool,
    pub(crate) max_depth: u32,
    pub(crate) no_content: bool,
    pub(crate) extensions: Option<Vec<String>>,
    pub(crate) include_hidden: bool,
}

pub(crate) fn handle(ctx: &CliContext, request: WalkRequest) -> Result<()> {
    use crate::walk::{WalkOptions, generate_manifest};

    if ctx.dry_run {
        if request.stdout {
            println!(
                "[dry-run] Generate manifest for {} -> stdout",
                request.directory.display()
            );
        } else {
            println!(
                "[dry-run] Generate manifest for {} -> {}",
                request.directory.display(),
                request.output.display()
            );
        }
        return Ok(());
    }

    let opts = WalkOptions {
        max_depth: request.max_depth as usize,
        include_content: !request.no_content,
        extensions: request.extensions,
        ignore_hidden: !request.include_hidden,
    };

    if !request.stdout {
        println!("Generating directory manifest...");
    }
    let manifest = generate_manifest(&request.directory, opts)?;

    if request.stdout {
        print!("{}", manifest);
    } else {
        std::fs::write(&request.output, manifest)?;

        println!(
            "Directory map generated successfully: {}",
            request.output.display()
        );
    }

    Ok(())
}
