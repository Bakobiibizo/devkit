use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

pub struct ReviewOptions {
    pub include_working: bool,
    pub compare_main: bool,
}

fn run_git(args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .args(args)
        .output()
        .context("Failed to execute git command")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git {} failed: {}", args.join(" "), stderr);
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn collect_file_diffs(diff_args: &[&str]) -> Result<Vec<(String, String)>> {
    let mut args = vec!["diff"];
    args.extend_from_slice(diff_args);
    args.push("--name-only");

    let names_output = run_git(&args)?;
    let paths: Vec<String> = names_output
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|s| s.to_string())
        .collect();

    let mut diffs = Vec::new();
    for path in paths {
        let mut diff_cmd = vec!["diff"];
        diff_cmd.extend_from_slice(diff_args);
        diff_cmd.push("--");
        diff_cmd.push(&path);

        let diff = run_git(&diff_cmd)?;
        if !diff.trim().is_empty() {
            diffs.push((path, diff));
        }
    }

    Ok(diffs)
}

#[derive(Debug)]
struct DiffHunk {
    header: String,
    content: Vec<String>,
    new_start: usize,
}

fn parse_hunks(diff_text: &str) -> Vec<DiffHunk> {
    let mut hunks = Vec::new();
    let mut current_header: Option<String> = None;
    let mut current_content: Vec<String> = Vec::new();
    let mut current_start = 1;

    for line in diff_text.lines() {
        if line.starts_with("@@") {
            if let Some(header) = current_header.take() {
                hunks.push(DiffHunk {
                    header,
                    content: current_content.clone(),
                    new_start: current_start,
                });
            }
            current_header = Some(line.to_string());
            current_content.clear();

            // Parse new file start line: @@ -a,b +c,d @@
            if let Some(new_seg) = line.split_whitespace().find(|s| s.starts_with('+')) {
                let new_start_raw = new_seg.trim_start_matches('+');
                if let Some(comma_pos) = new_start_raw.find(',') {
                    if let Ok(start) = new_start_raw[..comma_pos].parse() {
                        current_start = start;
                    }
                } else if let Ok(start) = new_start_raw.parse() {
                    current_start = start;
                }
            }
        } else if current_header.is_some() {
            current_content.push(line.to_string());
        }
    }

    if let Some(header) = current_header {
        hunks.push(DiffHunk {
            header,
            content: current_content,
            new_start: current_start,
        });
    }

    hunks
}

fn render_overlay(file_path: &str, diff: &str, repo_root: &Path) -> Vec<String> {
    let mut overlay = Vec::new();
    let file_lang = Path::new(file_path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    
    overlay.push(format!("```{}", file_lang));

    let target_path = repo_root.join(file_path);
    if !target_path.exists() {
        overlay.push("_File deleted; showing diff below._".to_string());
        overlay.extend(diff.lines().map(|s| s.to_string()));
        overlay.push("```".to_string());
        return overlay;
    }

    let file_lines = match std::fs::read_to_string(&target_path) {
        Ok(content) => content.lines().map(|s| s.to_string()).collect::<Vec<_>>(),
        Err(_) => {
            overlay.push("_Could not read file; showing diff below._".to_string());
            overlay.extend(diff.lines().map(|s| s.to_string()));
            overlay.push("```".to_string());
            return overlay;
        }
    };

    let hunks = parse_hunks(diff);
    let mut line_idx = 1;

    for hunk in hunks {
        // Add unchanged lines before this hunk
        while line_idx < hunk.new_start && line_idx <= file_lines.len() {
            overlay.push(file_lines[line_idx - 1].clone());
            line_idx += 1;
        }

        // Add a visual separator for the diff section
        overlay.push(String::new());
        overlay.push(format!(">>> CHANGES START {} <<<", hunk.header));
        
        // Process the diff hunk content
        for diff_line in &hunk.content {
            if diff_line.starts_with('+') && !diff_line.starts_with("+++") {
                // Added line - show with + prefix
                overlay.push(format!("+ {}", &diff_line[1..]));
                line_idx += 1;
            } else if diff_line.starts_with('-') && !diff_line.starts_with("---") {
                // Removed line - show with - prefix (don't increment line_idx)
                overlay.push(format!("- {}", &diff_line[1..]));
            } else if diff_line.starts_with(' ') {
                // Context line - show as-is
                overlay.push(diff_line[1..].to_string());
                line_idx += 1;
            }
        }
        
        overlay.push(">>> CHANGES END <<<".to_string());
        overlay.push(String::new());
    }

    // Add remaining unchanged lines
    while line_idx <= file_lines.len() {
        overlay.push(file_lines[line_idx - 1].clone());
        line_idx += 1;
    }

    overlay.push("```".to_string());

    overlay
}

fn render_section(title: &str, entries: &[(String, String)], repo_root: &Path) -> String {
    let mut lines = vec![format!("## {}", title)];
    
    if entries.is_empty() {
        lines.push("_No changes detected in this scope._".to_string());
        lines.push(String::new());
    } else {
        for (file_path, diff) in entries {
            lines.push(format!("### `{}`", file_path));
            lines.extend(render_overlay(file_path, diff, repo_root));
        }
    }

    lines.join("\n")
}

pub fn generate_review(opts: ReviewOptions, repo_root: &Path) -> Result<String> {
    let timestamp = chrono::Utc::now().format("%Y-%m-%d %H:%M:%SZ");
    let current_branch = run_git(&["rev-parse", "--abbrev-ref", "HEAD"])?.trim().to_string();
    let status = run_git(&["status", "-sb"])?;

    let mut sections = Vec::new();

    if opts.compare_main {
        let main_entries = collect_file_diffs(&["main...HEAD"])?;
        sections.push(render_section("Changes vs main", &main_entries, repo_root));
    } else {
        let staged_entries = collect_file_diffs(&["--cached"])?;
        sections.push(render_section("Staged Changes", &staged_entries, repo_root));

        if opts.include_working {
            let worktree_entries = collect_file_diffs(&[])?;
            sections.push(render_section("Unstaged Changes", &worktree_entries, repo_root));
        }
    }

    let header = format!(
        "# Code Review Overlay\n\n\
         _Generated at {} on branch `{}`_\n\n\
         ## Git Status\n\
         ```\n\
         {}\
         ```\n\n",
        timestamp, current_branch, status
    );

    Ok(header + &sections.join("\n"))
}

pub fn get_repo_root() -> Result<PathBuf> {
    let output = run_git(&["rev-parse", "--show-toplevel"])?;
    Ok(PathBuf::from(output.trim()))
}
