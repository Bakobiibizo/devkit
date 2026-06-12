use anyhow::Result;

use crate::cli::VersionCommand;
use crate::dispatch::AppState;
use crate::versioning;

pub(crate) fn handle(state: &AppState, command: VersionCommand) -> Result<()> {
    versioning::handle(&state.config, state.ctx.dry_run, command)
}
