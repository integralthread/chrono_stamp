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

fn git(directory: &Path, args: &[&str]) {
    let status = Command::new("git")
        .arg("-C")
        .arg(directory)
        .args(args)
        .status()
        .expect("git should run");
    assert!(status.success(), "git {args:?} should succeed");
}

fn fixture() -> TempDir {
    let directory = TempDir::new().expect("temporary project");
    fs::write(
        directory.path().join("Cargo.toml"),
        "# keep this comment\n[package]\nname = \"fixture\"\nversion = \"2026.8.0-rc\"\n",
    )
    .unwrap();
    fs::write(
        directory.path().join("package.json"),
        "{\"metadata\":{\"version\":\"2026.8.0-rc\"}}\n",
    )
    .unwrap();
    fs::write(
        directory.path().join("README.md"),
        "Current version: 2026.8.0-rc\n",
    )
    .unwrap();
    fs::write(
        directory.path().join(".chrono.toml"),
        r#"schema = 1
version_source = "Cargo.toml"

[[updates]]
path = "package.json"
kind = "json"
key = "metadata.version"

[[updates]]
path = "README.md"
kind = "regex"
pattern = '(?m)(?<prefix>Current version:\s*)[^\s]+(?<suffix>\s|$)'
expected_matches = 1
"#,
    )
    .unwrap();
    git(directory.path(), &["init", "--quiet"]);
    git(directory.path(), &["config", "user.name", "Chrono Test"]);
    git(
        directory.path(),
        &["config", "user.email", "chrono@example.invalid"],
    );
    git(directory.path(), &["add", "."]);
    git(directory.path(), &["commit", "--quiet", "-m", "initial"]);
    directory
}

#[test]
fn bump_dry_run_does_not_change_any_file() {
    let project = fixture();
    let paths = ["Cargo.toml", "package.json", "README.md"];
    let before = paths.map(|path| fs::read(project.path().join(path)).unwrap());

    let output = run(
        &["bump", "--date", "2026-08-27", "--dry-run"],
        project.path(),
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    for (path, expected) in paths.into_iter().zip(before) {
        assert_eq!(fs::read(project.path().join(path)).unwrap(), expected);
    }
}

#[test]
fn bump_promotes_and_updates_all_targets() {
    let project = fixture();
    let output = run(&["bump", "--date", "2026-08-27"], project.path());
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cargo = fs::read_to_string(project.path().join("Cargo.toml")).unwrap();
    let package = fs::read_to_string(project.path().join("package.json")).unwrap();
    let readme = fs::read_to_string(project.path().join("README.md")).unwrap();
    assert!(cargo.starts_with("# keep this comment"));
    assert!(cargo.contains("version = \"2026.8.0\""));
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&package).unwrap()["metadata"]["version"],
        "2026.8.0"
    );
    assert_eq!(readme, "Current version: 2026.8.0\n");

    let checked = run(&["check"], project.path());
    assert!(checked.status.success());
}

#[test]
fn inconsistent_target_stops_bump_before_any_write() {
    let project = fixture();
    fs::write(
        project.path().join("README.md"),
        "Current version: 2026.7.9\n",
    )
    .unwrap();
    let cargo_before = fs::read(project.path().join("Cargo.toml")).unwrap();
    let output = run(&["bump", "--date", "2026-08-27"], project.path());
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        fs::read(project.path().join("Cargo.toml")).unwrap(),
        cargo_before
    );
    assert!(String::from_utf8_lossy(&output.stderr).contains("version mismatch"));
}

#[test]
fn sync_repairs_secondary_files_without_touching_primary() {
    let project = fixture();
    fs::write(
        project.path().join("README.md"),
        "Current version: 2026.7.9\n",
    )
    .unwrap();
    let cargo_before = fs::read(project.path().join("Cargo.toml")).unwrap();
    let output = run(&["sync", "--allow-dirty-managed"], project.path());
    assert!(output.status.success());
    assert_eq!(
        fs::read(project.path().join("Cargo.toml")).unwrap(),
        cargo_before
    );
    assert_eq!(
        fs::read_to_string(project.path().join("README.md")).unwrap(),
        "Current version: 2026.8.0-rc\n"
    );
}

#[test]
fn sync_refuses_dirty_managed_paths_without_override() {
    let project = fixture();
    fs::write(
        project.path().join("README.md"),
        "Current version: 2026.7.9\n",
    )
    .unwrap();
    let output = run(&["sync"], project.path());
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("uncommitted changes"));
    assert_eq!(
        fs::read_to_string(project.path().join("README.md")).unwrap(),
        "Current version: 2026.7.9\n"
    );
}

#[test]
fn dev_is_print_only_unless_write_is_explicit() {
    let project = fixture();
    let before = fs::read(project.path().join("Cargo.toml")).unwrap();
    let printed = run(&["dev", "--date", "2026-08-27"], project.path());
    assert!(printed.status.success());
    assert_eq!(fs::read(project.path().join("Cargo.toml")).unwrap(), before);

    let written = run(&["dev", "--date", "2026-08-27", "--write"], project.path());
    assert!(
        written.status.success(),
        "{}",
        String::from_utf8_lossy(&written.stderr)
    );
    let manifest = fs::read_to_string(project.path().join("Cargo.toml")).unwrap();
    assert!(
        manifest.contains("version = \"2026.8.0-") && !manifest.contains("2026.8.0-rc"),
        "{manifest}"
    );
}

#[test]
fn init_is_non_destructive_by_default_and_supports_dry_run() {
    let directory = TempDir::new().unwrap();
    fs::write(
        directory.path().join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"2026.8.0\"\n",
    )
    .unwrap();
    let preview = run(&["init", "--dry-run"], directory.path());
    assert!(preview.status.success());
    assert!(!directory.path().join(".chrono.toml").exists());

    assert!(run(&["init"], directory.path()).status.success());
    let original = fs::read(directory.path().join(".chrono.toml")).unwrap();
    let refused = run(&["init"], directory.path());
    assert_eq!(refused.status.code(), Some(1));
    assert_eq!(
        fs::read(directory.path().join(".chrono.toml")).unwrap(),
        original
    );
    let preview_refused = run(&["init", "--dry-run"], directory.path());
    assert_eq!(preview_refused.status.code(), Some(1));
}

#[test]
fn generic_init_requires_and_writes_an_initial_version() {
    let directory = TempDir::new().unwrap();
    let missing = run(&["init", "--dry-run"], directory.path());
    assert_eq!(missing.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&missing.stderr).contains("--initial-version"));

    let preview = run(
        &["init", "--initial-version", "2026.8.0", "--dry-run"],
        directory.path(),
    );
    assert!(preview.status.success());
    assert!(!directory.path().join("VERSION").exists());

    let created = run(&["init", "--initial-version", "2026.8.0"], directory.path());
    assert!(created.status.success());
    assert_eq!(
        fs::read_to_string(directory.path().join("VERSION")).unwrap(),
        "2026.8.0\n"
    );
    assert!(directory.path().join(".chrono.toml").is_file());
}
