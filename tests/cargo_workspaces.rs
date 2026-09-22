use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use tempfile::TempDir;

fn run(args: &[&str], directory: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_chrono"))
        .args(args)
        .current_dir(directory)
        .output()
        .expect("chrono should run")
}

fn command(directory: &Path, program: &str, args: &[&str]) {
    let output = Command::new(program)
        .args(args)
        .current_dir(directory)
        .output()
        .unwrap_or_else(|error| panic!("{program} should run: {error}"));
    assert!(
        output.status.success(),
        "{program} {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn git(directory: &Path, args: &[&str]) {
    command(directory, "git", args);
}

fn initialize_git(directory: &Path) {
    git(directory, &["init", "--quiet"]);
    git(directory, &["config", "user.name", "Chrono Test"]);
    git(
        directory,
        &["config", "user.email", "chrono@example.invalid"],
    );
    git(directory, &["add", "."]);
    git(directory, &["commit", "--quiet", "-m", "initial"]);
}

fn standalone_fixture() -> TempDir {
    let directory = TempDir::new().unwrap();
    fs::create_dir_all(directory.path().join("src")).unwrap();
    fs::write(
        directory.path().join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"2026.8.0-rc\"\nedition = \"2024\"\n",
    )
    .unwrap();
    fs::write(directory.path().join("src/lib.rs"), "pub fn fixture() {}\n").unwrap();
    command(directory.path(), "cargo", &["generate-lockfile"]);
    initialize_git(directory.path());
    directory
}

fn workspace_fixture() -> TempDir {
    let directory = TempDir::new().unwrap();
    fs::create_dir_all(directory.path().join("member/src")).unwrap();
    fs::create_dir_all(directory.path().join("other/src")).unwrap();
    fs::write(
        directory.path().join("Cargo.toml"),
        "[workspace]\nmembers = [\"member\", \"other\"]\nresolver = \"3\"\n\n[workspace.package]\nversion = \"2026.8.0-rc\"\nedition = \"2024\"\n",
    )
    .unwrap();
    fs::write(
        directory.path().join("member/Cargo.toml"),
        "[package]\nname = \"member\"\nversion.workspace = true\nedition.workspace = true\n",
    )
    .unwrap();
    fs::write(
        directory.path().join("member/src/lib.rs"),
        "pub fn member() {}\n",
    )
    .unwrap();
    fs::write(
        directory.path().join("other/Cargo.toml"),
        "[package]\nname = \"other\"\nversion.workspace = true\nedition.workspace = true\n",
    )
    .unwrap();
    fs::write(
        directory.path().join("other/src/lib.rs"),
        "pub fn other() {}\n",
    )
    .unwrap();
    command(directory.path(), "cargo", &["generate-lockfile"]);
    initialize_git(directory.path());
    directory
}

#[test]
fn standalone_bump_updates_and_validates_cargo_lock() {
    let project = standalone_fixture();
    let output = run(&["bump", "--date", "2026-08-27"], project.path());
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let manifest = fs::read_to_string(project.path().join("Cargo.toml")).unwrap();
    let lock = fs::read_to_string(project.path().join("Cargo.lock")).unwrap();
    assert!(manifest.contains("version = \"2026.8.0\""));
    assert!(lock.contains("version = \"2026.8.0\""));
    command(project.path(), "cargo", &["check", "--locked"]);
}

#[test]
fn workspace_release_commits_the_owner_and_lock() {
    let project = workspace_fixture();
    let output = run(
        &["release", "--date", "2026-08-27"],
        &project.path().join("member"),
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let changed = Command::new("git")
        .args(["show", "--pretty=format:", "--name-only", "HEAD"])
        .current_dir(project.path())
        .output()
        .unwrap();
    let changed = String::from_utf8(changed.stdout).unwrap();
    assert!(changed.lines().any(|path| path == "Cargo.toml"));
    assert!(changed.lines().any(|path| path == "Cargo.lock"));
    assert_eq!(
        Command::new("git")
            .args(["status", "--porcelain"])
            .current_dir(project.path())
            .output()
            .unwrap()
            .stdout,
        b""
    );
}

#[test]
fn workspace_member_bump_updates_the_version_owner_and_lock() {
    let project = workspace_fixture();
    let member = project.path().join("member");
    let output = run(&["bump", "--date", "2026-08-27"], &member);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let root_manifest = fs::read_to_string(project.path().join("Cargo.toml")).unwrap();
    let member_manifest = fs::read_to_string(member.join("Cargo.toml")).unwrap();
    let lock = fs::read_to_string(project.path().join("Cargo.lock")).unwrap();
    assert!(root_manifest.contains("version = \"2026.8.0\""));
    assert!(member_manifest.contains("version.workspace = true"));
    assert!(lock.contains("version = \"2026.8.0\""));
    assert_eq!(lock.matches("version = \"2026.8.0\"").count(), 2);
    command(project.path(), "cargo", &["check", "--locked"]);
}

#[test]
fn workspace_dry_run_changes_neither_owner_nor_lock() {
    let project = workspace_fixture();
    let manifest = fs::read(project.path().join("Cargo.toml")).unwrap();
    let lock = fs::read(project.path().join("Cargo.lock")).unwrap();
    let output = run(
        &["bump", "--date", "2026-08-27", "--dry-run"],
        &project.path().join("member"),
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        fs::read(project.path().join("Cargo.toml")).unwrap(),
        manifest
    );
    assert_eq!(fs::read(project.path().join("Cargo.lock")).unwrap(), lock);
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("Cargo.toml"));
    assert!(stdout.contains("Cargo.lock"));
}

#[test]
fn virtual_workspace_root_resolves_the_shared_version() {
    let project = workspace_fixture();
    let output = run(
        &["--format", "json", "status", "--date", "2026-08-27"],
        project.path(),
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["version"], "2026.8.0-rc");
    assert_eq!(value["next"], "2026.8.0");
}
