use anyhow::Result;

use crate::cli::VaultCommand;
use crate::dispatch::AppState;
use crate::vault;

pub(crate) fn handle(state: &AppState, command: VaultCommand) -> Result<()> {
    match command {
        VaultCommand::List { account } => vault::list_items(&account, state.ctx.dry_run),
        VaultCommand::Get {
            item,
            field,
            account,
        } => vault::get_item(&account, &item, field.as_deref(), state.ctx.dry_run),
        VaultCommand::Set {
            item,
            value,
            account,
        } => vault::set_item(&account, &item, &value, state.ctx.dry_run),
        VaultCommand::Delete { item, account } => {
            vault::delete_item(&account, &item, state.ctx.dry_run)
        }
    }
}
