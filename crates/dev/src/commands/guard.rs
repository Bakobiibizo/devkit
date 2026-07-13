use anyhow::Result;

use crate::cli::GuardArgs;
use crate::dispatch::CliContext;
use crate::guard::{GuardOptions, run_guard};

pub(crate) fn handle(ctx: &CliContext, args: GuardArgs) -> Result<()> {
    if ctx.dry_run {
        println!(
            "[dry-run] Check added lines in {}...{} with {}",
            args.base,
            args.head,
            args.config.display()
        );
        return Ok(());
    }

    run_guard(GuardOptions {
        base: args.base,
        head: args.head,
        config: args.config,
        format: args.format,
        rules_from_worktree: args.rules_from_worktree,
    })
}
