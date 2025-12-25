//! Environment variable file parsing

use std::collections::BTreeMap;
use std::path::PathBuf;

/// Load environment variables from ~/.env
pub fn load_env_vars() -> Vec<(String, String)> {
    let env_path = find_env_file();

    let content = match std::fs::read_to_string(&env_path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

    parse_env_content(&content)
}

/// Find the .env file - check current directory first, then home directory
fn find_env_file() -> PathBuf {
    // Check current directory
    let cwd_env = std::env::current_dir()
        .map(|d| d.join(".env"))
        .unwrap_or_else(|_| PathBuf::from(".env"));

    if cwd_env.exists() {
        return cwd_env;
    }

    // Fall back to home directory
    dirs::home_dir()
        .map(|h| h.join(".env"))
        .unwrap_or_else(|| PathBuf::from("~/.env"))
}

/// Parse .env file content into key-value pairs
fn parse_env_content(content: &str) -> Vec<(String, String)> {
    let mut vars: BTreeMap<String, String> = BTreeMap::new();

    for line in content.lines() {
        let line = line.trim();

        // Skip empty lines and comments
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        // Parse KEY=VALUE
        if let Some((key, value)) = line.split_once('=') {
            let key = key.trim().to_string();
            let mut value = value.trim().to_string();

            // Remove surrounding quotes if present
            if (value.starts_with('"') && value.ends_with('"'))
                || (value.starts_with('\'') && value.ends_with('\''))
            {
                value = value[1..value.len() - 1].to_string();
            }

            if !key.is_empty() {
                vars.insert(key, value);
            }
        }
    }

    vars.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_env() {
        let content = r#"
# This is a comment
API_KEY=secret123
DATABASE_URL="postgres://localhost/db"
EMPTY=
QUOTED='single quoted'
"#;
        let vars = parse_env_content(content);
        assert_eq!(vars.len(), 4);
        assert_eq!(vars[0], ("API_KEY".to_string(), "secret123".to_string()));
        assert_eq!(
            vars[1],
            ("DATABASE_URL".to_string(), "postgres://localhost/db".to_string())
        );
    }
}
