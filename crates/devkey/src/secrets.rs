//! 1Password secrets integration for devkey
//!
//! Fetches password items from production and development vaults.

#[cfg(windows)]
use std::os::windows::process::CommandExt;
use std::process::Command;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;

/// A secret item from 1Password
#[derive(Debug, Clone)]
pub struct SecretItem {
    pub title: String,
    pub id: String,
}

/// Load the service account token for a vault from ~/.env
fn load_token(account: &str) -> Option<String> {
    let home = dirs::home_dir()?;
    let env_path = home.join(".env");

    if !env_path.exists() {
        return None;
    }

    let content = std::fs::read_to_string(&env_path).ok()?;
    let key = format!("OP_{}", account.to_uppercase());

    for line in content.lines() {
        let line = line.trim();
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            if k.trim() == key {
                let value = v.trim().trim_matches('"').trim_matches('\'');
                return Some(value.to_string());
            }
        }
    }

    None
}

/// List password items from a vault
pub fn list_secrets(account: &str) -> Vec<SecretItem> {
    let Some(token) = load_token(account) else {
        return Vec::new();
    };

    let mut cmd = Command::new("op");
    cmd.env("OP_SERVICE_ACCOUNT_TOKEN", &token).args([
        "item",
        "list",
        "--vault",
        account,
        "--categories",
        "Password",
        "--format",
        "json",
    ]);

    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);

    let output = cmd.output();

    let output = match output {
        Ok(o) if o.status.success() => o.stdout,
        _ => return Vec::new(),
    };

    let items: serde_json::Value = match serde_json::from_slice(&output) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };

    let Some(arr) = items.as_array() else {
        return Vec::new();
    };

    arr.iter()
        .filter_map(|item| {
            let title = item.get("title")?.as_str()?.to_string();
            let id = item.get("id")?.as_str()?.to_string();
            Some(SecretItem { title, id })
        })
        .collect()
}

/// Get the password value for a secret item
pub fn get_secret_value(account: &str, item_id: &str) -> Option<String> {
    let token = load_token(account)?;

    let mut cmd = Command::new("op");
    cmd.env("OP_SERVICE_ACCOUNT_TOKEN", &token).args([
        "item",
        "get",
        item_id,
        "--vault",
        account,
        "--fields",
        "label=password",
    ]);

    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);

    let output = cmd.output().ok()?;

    if !output.status.success() {
        return None;
    }

    Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
}
