use std::fs;
use std::path::Path;
use std::process::Command;

use assert_cmd::cargo::cargo_bin_cmd;
use tempfile::TempDir;

fn dev() -> assert_cmd::Command {
    cargo_bin_cmd!("dev")
}

fn write_file(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create parent directory");
    }
    fs::write(path, contents).expect("write test file");
}

fn run_git(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap_or_else(|error| panic!("run git {args:?}: {error}"));
    assert!(
        output.status.success(),
        "git {args:?} failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

fn init_repo(dir: &Path) {
    run_git(dir, &["init", "-b", "main"]);
    run_git(dir, &["config", "user.email", "devkit@example.invalid"]);
    run_git(dir, &["config", "user.name", "Devkit Tests"]);
}

#[test]
fn config_generate_check_and_task_run_round_trip() {
    let temp = TempDir::new().expect("tempdir");
    let project = temp.path();

    dev()
        .args([
            "-C",
            project.to_str().unwrap(),
            "config",
            "generate",
            ".dev/config.toml",
        ])
        .assert()
        .success()
        .stdout(predicates::str::contains("Wrote example config"));

    let config_path = project.join(".dev/config.toml");
    assert!(config_path.exists(), "config was generated");

    dev()
        .args(["-C", project.to_str().unwrap(), "config", "check"])
        .assert()
        .success()
        .stdout(predicates::str::contains("Config OK"));

    dev()
        .args([
            "-C",
            project.to_str().unwrap(),
            "config",
            "add",
            "touch_marker",
            "touch",
            "marker.txt",
        ])
        .assert()
        .success()
        .stdout(predicates::str::contains("Wrote task `touch_marker`"));

    dev()
        .args(["-C", project.to_str().unwrap(), "run", "touch_marker"])
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "Task `touch_marker` completed successfully.",
        ));

    assert!(project.join("marker.txt").exists(), "task created marker");
}

#[test]
fn task_refs_flatten_and_cycles_fail_before_execution() {
    let temp = TempDir::new().expect("tempdir");
    let project = temp.path();
    write_file(
        &project.join(".dev/config.toml"),
        r#"
[tasks.leaf]
commands = [["touch", "leaf.txt"]]

[tasks.parent]
commands = ["leaf"]
"#,
    );

    dev()
        .args(["-C", project.to_str().unwrap(), "run", "parent"])
        .assert()
        .success();
    assert!(project.join("leaf.txt").exists(), "nested task ran");

    write_file(
        &project.join(".dev/config.toml"),
        r#"
[tasks.a]
commands = ["b"]

[tasks.b]
commands = ["a", ["touch", "should-not-exist"]]
"#,
    );

    dev()
        .args(["-C", project.to_str().unwrap(), "run", "a"])
        .assert()
        .failure()
        .stderr(predicates::str::contains(
            "task recursion detected: a -> b -> a",
        ));
    assert!(
        !project.join("should-not-exist").exists(),
        "cycle stopped execution"
    );
}

#[test]
fn env_file_ops_cover_values_profiles_switch_and_remove() {
    let temp = TempDir::new().expect("tempdir");
    let project = temp.path();

    dev()
        .args([
            "-C",
            project.to_str().unwrap(),
            "env",
            "add",
            "API_URL",
            "http://localhost:3000",
        ])
        .assert()
        .success();

    dev()
        .args(["-C", project.to_str().unwrap(), "env", "get", "API_URL"])
        .assert()
        .success()
        .stdout("http://localhost:3000\n");

    dev()
        .args(["-C", project.to_str().unwrap(), "env", "save", "local"])
        .assert()
        .success();

    dev()
        .args([
            "-C",
            project.to_str().unwrap(),
            "env",
            "add",
            "API_URL",
            "http://staging.invalid",
        ])
        .assert()
        .success();

    dev()
        .args(["-C", project.to_str().unwrap(), "env", "profiles"])
        .assert()
        .success()
        .stdout(predicates::str::contains("local"));

    dev()
        .args(["-C", project.to_str().unwrap(), "env", "switch", "local"])
        .assert()
        .success();

    assert_eq!(
        fs::read_to_string(project.join(".env")).expect("read .env"),
        "API_URL=http://localhost:3000"
    );

    dev()
        .args(["-C", project.to_str().unwrap(), "env", "rm", "API_URL"])
        .assert()
        .success();

    let env_contents = fs::read_to_string(project.join(".env")).expect("read .env");
    assert!(!env_contents.contains("API_URL="), "key was removed");

    dev()
        .args(["-C", project.to_str().unwrap(), "env", "get", "API_URL"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("key `API_URL` not found"));
}

#[test]
fn git_branch_create_and_finalize_work_without_remote_or_release_candidate() {
    let temp = TempDir::new().expect("tempdir");
    let repo = temp.path();
    init_repo(repo);
    write_file(&repo.join("README.md"), "initial\n");
    run_git(repo, &["add", "README.md"]);
    run_git(repo, &["commit", "-m", "initial commit"]);

    dev()
        .args([
            "-C",
            repo.to_str().unwrap(),
            "git",
            "branch-create",
            "feature/local-flow",
        ])
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "Branch `feature/local-flow` created from `main`.",
        ));

    assert_eq!(
        run_git(repo, &["branch", "--show-current"]),
        "feature/local-flow"
    );
    assert!(
        run_git(repo, &["branch", "--list", "release-candidate"]).is_empty(),
        "release-candidate was not created"
    );

    write_file(&repo.join("README.md"), "initial\nfeature\n");
    run_git(repo, &["add", "README.md"]);
    run_git(repo, &["commit", "-m", "feature work"]);

    dev()
        .args(["-C", repo.to_str().unwrap(), "git", "branch-finalize"])
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "Finalized `feature/local-flow` into `main`.",
        ));

    assert_eq!(run_git(repo, &["branch", "--show-current"]), "main");
    assert_eq!(
        fs::read_to_string(repo.join("README.md")).expect("read README"),
        "initial\nfeature\n"
    );
    assert_eq!(
        run_git(repo, &["log", "-1", "--pretty=%s"]),
        "Merge branch `feature/local-flow` into `main`"
    );
}

#[test]
fn version_bump_updates_cargo_changelog_and_git_history() {
    let temp = TempDir::new().expect("tempdir");
    let project = temp.path();
    init_repo(project);
    write_file(
        &project.join("Cargo.toml"),
        r#"[package]
name = "temp-project"
version = "0.1.0"
edition = "2024"
"#,
    );
    write_file(&project.join("CHANGELOG.md"), "# Changelog\n\n");
    run_git(project, &["add", "Cargo.toml", "CHANGELOG.md"]);
    run_git(project, &["commit", "-m", "initial project"]);

    dev()
        .args(["-C", project.to_str().unwrap(), "version", "bump", "patch"])
        .assert()
        .success()
        .stdout(predicates::str::contains("Updated"));

    let manifest = fs::read_to_string(project.join("Cargo.toml")).expect("read manifest");
    assert!(manifest.contains(r#"version = "0.1.1""#));

    let changelog = fs::read_to_string(project.join("CHANGELOG.md")).expect("read changelog");
    assert!(changelog.contains("v0.1.1"));
    assert!(changelog.contains("- Describe the notable changes here."));

    assert_eq!(
        run_git(project, &["log", "-1", "--pretty=%s"]),
        "chore: release 0.1.1"
    );
    assert!(
        run_git(project, &["status", "--porcelain"]).is_empty(),
        "version bump left a clean worktree"
    );
}
