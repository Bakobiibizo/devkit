use anyhow::Result;
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::time::SystemTime;

pub struct WalkOptions {
    pub max_depth: usize,
    pub include_content: bool,
    pub extensions: Option<Vec<String>>,
    pub ignore_hidden: bool,
}

impl Default for WalkOptions {
    fn default() -> Self {
        Self {
            max_depth: 10,
            include_content: true,
            extensions: None,
            ignore_hidden: true,
        }
    }
}

fn get_ignore_patterns() -> HashSet<&'static str> {
    let mut patterns = HashSet::new();
    // General/OS
    patterns.insert(".DS_Store");
    patterns.insert("Thumbs.db");
    patterns.insert("nohup.out");
    patterns.insert(".cache");
    // Git and env
    patterns.insert(".git");
    patterns.insert(".gitmodules");
    patterns.insert(".env");
    patterns.insert(".env.local");
    patterns.insert(".env.production");
    patterns.insert(".env.development");
    // Python
    patterns.insert("__pycache__");
    patterns.insert(".venv");
    patterns.insert("venv");
    patterns.insert(".mypy_cache");
    patterns.insert(".pytest_cache");
    patterns.insert(".ruff_cache");
    patterns.insert(".tox");
    patterns.insert("pip-wheel-metadata");
    patterns.insert(".ipynb_checkpoints");
    patterns.insert("dist");
    patterns.insert("build");
    patterns.insert(".coverage");
    patterns.insert("htmlcov");
    // Node/TypeScript
    patterns.insert("node_modules");
    patterns.insert("pnpm-lock.yaml");
    patterns.insert("yarn.lock");
    patterns.insert("package-lock.json");
    patterns.insert(".eslintcache");
    patterns.insert(".tsbuildinfo");
    patterns.insert(".pnpm-store");
    patterns.insert(".turbo");
    patterns.insert(".parcel-cache");
    patterns.insert(".vite");
    patterns.insert(".next");
    patterns.insert(".nuxt");
    patterns.insert(".svelte-kit");
    patterns.insert("storybook-static");
    patterns.insert("coverage");
    patterns.insert(".nyc_output");
    patterns.insert(".vercel");
    patterns.insert(".netlify");
    patterns.insert(".docusaurus");
    patterns.insert("out");
    // Rust
    patterns.insert("target");
    patterns.insert("Cargo.lock");
    patterns.insert(".cargo");
    // Solidity/Foundry/Hardhat
    patterns.insert("artifacts");
    patterns.insert("typechain");
    patterns.insert("broadcast");
    patterns.insert("abi");
    patterns.insert("abis");
    // Project-specific
    patterns.insert(".lock");
    patterns.insert("manifest.md");
    patterns.insert("manifest-2.md");
    // IDEs
    patterns.insert(".vscode");
    patterns.insert(".idea");
    patterns
}

fn should_ignore(name: &str, ignore_hidden: bool, patterns: &HashSet<&str>) -> bool {
    if ignore_hidden && name.starts_with('.') {
        return true;
    }
    patterns.iter().any(|pattern| name.contains(pattern))
}

fn format_timestamp(time: SystemTime) -> String {
    use std::time::UNIX_EPOCH;
    if let Ok(duration) = time.duration_since(UNIX_EPOCH) {
        let secs = duration.as_secs();
        let datetime = chrono::DateTime::<chrono::Utc>::from_timestamp(secs as i64, 0);
        if let Some(dt) = datetime {
            return dt.format("%Y-%m-%d %H:%M:%S").to_string();
        }
    }
    "unknown".to_string()
}

fn walk_directory(
    path: &Path,
    output: &mut String,
    depth: usize,
    opts: &WalkOptions,
    patterns: &HashSet<&str>,
) -> Result<()> {
    if depth >= opts.max_depth {
        return Ok(());
    }

    let indent = "  ".repeat(depth);
    
    let mut entries: Vec<_> = fs::read_dir(path)?
        .filter_map(|e| e.ok())
        .collect();
    
    entries.sort_by_key(|e| e.file_name());

    for entry in entries {
        let file_name = entry.file_name();
        let name = file_name.to_string_lossy();
        
        if should_ignore(&name, opts.ignore_hidden, patterns) {
            continue;
        }

        let entry_path = entry.path();
        let metadata = entry.metadata()?;

        if metadata.is_dir() {
            output.push_str(&format!("{}- 📁 **{}/**\n", indent, name));
            walk_directory(&entry_path, output, depth + 1, opts, patterns)?;
        } else {
            output.push_str(&format!("{}- 📄 **{}**\n", indent, name));
            
            if opts.include_content {
                let ext = entry_path.extension()
                    .and_then(|e| e.to_str())
                    .map(|e| format!(".{}", e));
                
                let should_include = if let Some(ref exts) = opts.extensions {
                    ext.as_ref().map_or(false, |e| exts.contains(e))
                } else {
                    true
                };

                if should_include {
                    if let Ok(content) = fs::read_to_string(&entry_path) {
                        let size = metadata.len();
                        let modified = metadata.modified()
                            .map(format_timestamp)
                            .unwrap_or_else(|_| "unknown".to_string());
                        
                        output.push_str(&format!("\n{}  📄 *File Path*: `{}`\n", indent, entry_path.display()));
                        output.push_str(&format!("{}  *Size*: {} bytes | *Modified*: {}\n\n", indent, size, modified));
                        output.push_str(&format!("{}  ```\n", indent));
                        for line in content.lines() {
                            output.push_str(&format!("{}  {}\n", indent, line));
                        }
                        output.push_str(&format!("{}  ```\n\n", indent));
                    }
                }
            }
        }
    }

    Ok(())
}

pub fn generate_manifest(dir: &Path, opts: WalkOptions) -> Result<String> {
    let mut output = String::from("# Directory Structure\n\n");
    
    let dir_name = dir.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(".");
    
    output.push_str(&format!("- 📁 **{}/**\n", dir_name));
    
    let patterns = get_ignore_patterns();
    walk_directory(dir, &mut output, 1, &opts, &patterns)?;
    
    Ok(output)
}
