use anyhow::Result;

use crate::cli::VersionCommand;

pub fn handle(command: VersionCommand) -> Result<()> {
    let _ = command;
    // TODO: implement version bumping, tagging, and changelog generation.
    Ok(())
}
