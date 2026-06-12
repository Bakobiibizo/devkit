use anyhow::Result;

use crate::cli::LanguageCommand;
use crate::config;
use crate::dispatch::{AppState, CliContext};

pub(crate) fn handle(state: &AppState, command: LanguageCommand) -> Result<()> {
    match command {
        LanguageCommand::Set { name } => handle_set(&state.ctx, name),
    }
}

pub(crate) fn handle_set(ctx: &CliContext, name: String) -> Result<()> {
    let resolved = ctx.resolve_config_path()?;
    let path = resolved.path;
    config::set_default_language(&path, &name)?;
    println!(
        "Default language set to `{}` in {} ({})",
        name,
        path,
        resolved.source.as_str()
    );
    println!("Reload config to apply for this session.");
    Ok(())
}
