use anyhow::Result;

use crate::cli::GitCommand;
use crate::dispatch::AppState;
use crate::gitops;

pub(crate) fn handle(state: &AppState, command: GitCommand) -> Result<()> {
    match command {
        GitCommand::BranchCreate(args) => {
            gitops::branch_create(&args, state.ctx.dry_run, &state.config)
        }
        GitCommand::BranchFinalize(args) => {
            gitops::branch_finalize(&args, state.ctx.dry_run, &state.config)
        }
        GitCommand::ReleasePr(args) => gitops::release_pr(&args, state.ctx.dry_run, &state.config),
    }
}
