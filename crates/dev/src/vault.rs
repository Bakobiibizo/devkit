//! 1Password vault integration for managing secrets.
//!
//! Uses service account tokens from ~/.env:
//! - OP_PRODUCTION for production vault
//! - OP_DEVELOPMENT for development vault

use anyhow::{bail, Context, Result};
use std::process::Command;

/// Load the service account token for a vault from ~/.env
fn load_token(account: &str) -> Result<String> {
    let home = dirs::home_dir().context("Could not find home directory")?;
    let env_path = home.join(".env");

    if !env_path.exists() {
        bail!("~/.env not found. Create it with OP_PRODUCTION and OP_DEVELOPMENT tokens.");
    }

    let content = std::fs::read_to_string(&env_path)?;
    let key = format!("OP_{}", account.to_uppercase());

    for line in content.lines() {
        let line = line.trim();
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            if k.trim() == key {
                // Remove surrounding quotes if present
                let value = v.trim().trim_matches('"').trim_matches('\'');
                return Ok(value.to_string());
            }
        }
    }

    bail!(
        "Token {} not found in ~/.env. Add it as {}=<your-service-account-token>",
        key,
        key
    )
}

/// Run an op CLI command with the service account token
fn run_op(account: &str, args: &[&str]) -> Result<String> {
    let token = load_token(account)?;

    let output = Command::new("op")
        .env("OP_SERVICE_ACCOUNT_TOKEN", &token)
        .args(args)
        .output()
        .context("Failed to run op command. Is the 1Password CLI installed?")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("op command failed: {}", stderr);
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// List all password items in a vault
pub fn list_items(account: &str, dry_run: bool) -> Result<()> {
    if dry_run {
        println!("[dry-run] op item list --vault {} --categories Password --format json", account);
        return Ok(());
    }

    let output = run_op(
        account,
        &[
            "item",
            "list",
            "--vault",
            account,
            "--categories",
            "Password",
            "--format",
            "json",
        ],
    )?;

    // Parse JSON and display nicely
    let items: serde_json::Value = serde_json::from_str(&output)
        .context("Failed to parse op output as JSON")?;

    if let Some(arr) = items.as_array() {
        if arr.is_empty() {
            println!("No password items found in {} vault.", account);
            return Ok(());
        }

        println!("Secrets in {} vault:", account);
        for item in arr {
            if let Some(title) = item.get("title").and_then(|v| v.as_str()) {
                let id = item.get("id").and_then(|v| v.as_str()).unwrap_or("?");
                println!("  {} ({})", title, id);
            }
        }
    }

    Ok(())
}

/// Get a secret value from the vault
pub fn get_item(account: &str, item: &str, field: Option<&str>, dry_run: bool) -> Result<()> {
    if dry_run {
        if let Some(f) = field {
            println!("[dry-run] op item get {} --vault {} --fields label={}", item, account, f);
        } else {
            println!("[dry-run] op item get {} --vault {} --fields label=password", item, account);
        }
        return Ok(());
    }

    let field_name = field.unwrap_or("password");
    let output = run_op(
        account,
        &[
            "item",
            "get",
            item,
            "--vault",
            account,
            "--fields",
            &format!("label={}", field_name),
        ],
    )?;

    print!("{}", output.trim());
    Ok(())
}

/// Create or update a secret in the vault
pub fn set_item(account: &str, item: &str, value: &str, dry_run: bool) -> Result<()> {
    if dry_run {
        println!("[dry-run] op item create --vault {} --category Password --title {} password=<hidden>", account, item);
        return Ok(());
    }

    // First try to get the item to see if it exists
    let exists = run_op(
        account,
        &["item", "get", item, "--vault", account],
    ).is_ok();

    if exists {
        // Update existing item
        run_op(
            account,
            &[
                "item",
                "edit",
                item,
                "--vault",
                account,
                &format!("password={}", value),
            ],
        )?;
        println!("[ok] Updated secret: {}", item);
    } else {
        // Create new item
        run_op(
            account,
            &[
                "item",
                "create",
                "--vault",
                account,
                "--category",
                "Password",
                "--title",
                item,
                &format!("password={}", value),
            ],
        )?;
        println!("[ok] Created secret: {}", item);
    }

    Ok(())
}

/// Delete a secret from the vault
pub fn delete_item(account: &str, item: &str, dry_run: bool) -> Result<()> {
    if dry_run {
        println!("[dry-run] op item delete {} --vault {}", item, account);
        return Ok(());
    }

    run_op(account, &["item", "delete", item, "--vault", account])?;
    println!("[ok] Deleted secret: {}", item);
    Ok(())
}
