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

fn git_output(directory: &Path, args: &[&str]) -> Output {
    Command::new("git")
        .arg("-C")
        .arg(directory)
        .args(args)
        .output()
        .expect("git should run")
}

fn git(directory: &Path, args: &[&str]) {
    let output = git_output(directory, args);
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn fixture() -> TempDir {
    let directory = TempDir::new().expect("temporary project");
    fs::write(
        directory.path().join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"2026.8.0-rc\"\n",
    )
    .unwrap();
    fs::write(
        directory.path().join(".chrono.toml"),
        "schema = 1\nversion_source = \"Cargo.toml\"\n",
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

fn tag_exists(directory: &Path, name: &str) -> bool {
    git_output(
        directory,
        &[
            "show-ref",
            "--verify",
            "--quiet",
            &format!("refs/tags/{name}"),
        ],
    )
    .status
    .success()
}

#[test]
fn tag_dry_run_is_inert_and_duplicate_tags_are_refused() {
    let project = fixture();
    let preview = run(&["tag", "--dry-run"], project.path());
    assert!(preview.status.success());
    assert!(!tag_exists(project.path(), "v2026.8.0-rc"));

    let created = run(&["tag"], project.path());
    assert!(
        created.status.success(),
        "{}",
        String::from_utf8_lossy(&created.stderr)
    );
    assert!(tag_exists(project.path(), "v2026.8.0-rc"));
    let duplicate = run(&["tag"], project.path());
    assert_eq!(duplicate.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&duplicate.stderr).contains("already represented"));
}

#[test]
fn tag_refuses_normalized_aliases_and_invalid_prefixed_tags() {
    let aliases = fixture();
    git(aliases.path(), &["tag", "v2026.8-rc"]);
    let duplicate = run(&["tag"], aliases.path());
    assert_eq!(duplicate.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&duplicate.stderr).contains("already represented"));

    let invalid = fixture();
    git(invalid.path(), &["tag", "vlegacy"]);
    let output = run(
        &["release", "--date", "2026-08-27", "--dry-run"],
        invalid.path(),
    );
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("invalid ChronoStamp tags"));
}

#[test]
fn tag_requires_a_clean_tree() {
    let project = fixture();
    fs::write(project.path().join("notes.txt"), "dirty\n").unwrap();
    let output = run(&["tag"], project.path());
    assert_eq!(output.status.code(), Some(1));
    assert!(!tag_exists(project.path(), "v2026.8.0-rc"));
}

#[test]
fn release_dry_run_changes_neither_files_nor_git() {
    let project = fixture();
    let manifest = fs::read(project.path().join("Cargo.toml")).unwrap();
    let head = git_output(project.path(), &["rev-parse", "HEAD"]).stdout;
    let output = run(
        &["release", "--date", "2026-08-27", "--dry-run"],
        project.path(),
    );
    assert!(output.status.success());
    assert_eq!(
        fs::read(project.path().join("Cargo.toml")).unwrap(),
        manifest
    );
    assert_eq!(
        git_output(project.path(), &["rev-parse", "HEAD"]).stdout,
        head
    );
    assert!(!tag_exists(project.path(), "v2026.8.0"));
}

#[test]
fn release_updates_commits_and_tags_only_managed_files() {
    let project = fixture();
    let output = run(&["release", "--date", "2026-08-27"], project.path());
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        fs::read_to_string(project.path().join("Cargo.toml"))
            .unwrap()
            .contains("version = \"2026.8.0\"")
    );
    assert!(tag_exists(project.path(), "v2026.8.0"));
    assert_eq!(
        String::from_utf8(git_output(project.path(), &["status", "--porcelain"]).stdout).unwrap(),
        ""
    );
    assert_eq!(
        String::from_utf8(
            git_output(project.path(), &["show", "-s", "--format=%s", "HEAD"]).stdout
        )
        .unwrap(),
        "Release 2026.8.0\n"
    );
}

#[test]
fn release_pushes_head_and_tag_to_a_local_bare_remote() {
    let project = fixture();
    let remote = TempDir::new().unwrap();
    git(remote.path(), &["init", "--bare", "--quiet"]);
    git(
        project.path(),
        &["remote", "add", "origin", remote.path().to_str().unwrap()],
    );
    let output = run(
        &["release", "--date", "2026-08-27", "--push"],
        project.path(),
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(tag_exists(remote.path(), "v2026.8.0"));
}

#[test]
fn push_failure_preserves_the_local_release_and_reports_recovery() {
    let project = fixture();
    let output = run(
        &[
            "release",
            "--date",
            "2026-08-27",
            "--push",
            "--remote",
            "missing",
        ],
        project.path(),
    );
    assert_eq!(output.status.code(), Some(1));
    assert!(tag_exists(project.path(), "v2026.8.0"));
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(error.contains("recovery:"), "{error}");
    assert!(error.contains("safe locally"), "{error}");
}

fn json(output: &Output) -> serde_json::Value {
    serde_json::from_slice(&output.stdout).expect("valid JSON")
}

fn check_at_tag(directory: &Path) -> Output {
    run(&["--format", "json", "check", "--at-tag"], directory)
}

#[test]
fn check_at_tag_prints_the_release_tag_at_head() {
    let project = fixture();
    assert!(run(&["tag"], project.path()).status.success());

    let quiet = run(&["check", "--at-tag", "--quiet"], project.path());
    assert!(
        quiet.status.success(),
        "{}",
        String::from_utf8_lossy(&quiet.stderr)
    );
    assert_eq!(String::from_utf8(quiet.stdout).unwrap(), "v2026.8.0-rc\n");

    let output = check_at_tag(project.path());
    assert!(output.status.success());
    let value = json(&output);
    assert_eq!(value["schema_version"], 2);
    assert_eq!(value["version"], "2026.8.0-rc");
    assert_eq!(value["tag"], "v2026.8.0-rc");
    assert_eq!(value["annotated"], true);
    assert_eq!(value["hash"].as_str().unwrap().len(), 7);
}

#[test]
fn check_at_tag_matches_normalized_lightweight_aliases() {
    let project = fixture();
    git(project.path(), &["tag", "v2026.8-rc"]);
    let value = json(&check_at_tag(project.path()));
    assert_eq!(value["ok"], true);
    assert_eq!(value["tag"], "v2026.8-rc");
    assert_eq!(value["annotated"], false);
}

#[test]
fn check_at_tag_explains_missing_tags() {
    let project = fixture();
    let output = check_at_tag(project.path());
    assert_eq!(output.status.code(), Some(1));
    let value = json(&output);
    assert_eq!(value["error"]["code"], "head_untagged");
    assert!(
        value["error"]["message"]
            .as_str()
            .unwrap()
            .contains("no tags found locally")
    );

    git(
        project.path(),
        &["commit", "--quiet", "--allow-empty", "-m", "unreleased"],
    );
    git(project.path(), &["tag", "v2026.7.0", "HEAD~1"]);
    let value = json(&check_at_tag(project.path()));
    assert!(
        value["error"]["message"]
            .as_str()
            .unwrap()
            .contains("chrono tag")
    );

    git(project.path(), &["tag", "v2026.8.0-rc"]);
    git(
        project.path(),
        &["commit", "--quiet", "--allow-empty", "-m", "after release"],
    );
    let output = run(&["check", "--at-tag"], project.path());
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("has no v* tag"), "{stderr}");
    assert!(stderr.contains("chrono release"), "{stderr}");
}

#[test]
fn check_at_tag_rejects_extra_or_mismatched_tags() {
    let project = fixture();
    git(project.path(), &["tag", "v2026.8.0-rc"]);
    git(project.path(), &["tag", "v2026.8.0-beta"]);
    let value = json(&check_at_tag(project.path()));
    assert_eq!(value["error"]["code"], "head_tag_mismatch");
    assert_eq!(
        value["error"]["message"],
        "HEAD tags (v2026.8.0-rc,v2026.8.0-beta) do not match Cargo.toml version 2026.8.0-rc \
         (expected only v2026.8.0-rc)"
    );

    let invalid = fixture();
    git(invalid.path(), &["tag", "v2026.8.0-rc"]);
    git(invalid.path(), &["tag", "vlegacy"]);
    let value = json(&check_at_tag(invalid.path()));
    assert_eq!(value["error"]["code"], "git_preflight_error");
}

#[test]
fn check_at_tag_refuses_managed_files_that_differ_from_head() {
    let project = fixture();
    git(project.path(), &["tag", "v2026.8.0-rc"]);
    fs::write(
        project.path().join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"2026.8.0-rc\"\n# edited\n",
    )
    .unwrap();
    let value = json(&check_at_tag(project.path()));
    assert_eq!(value["error"]["code"], "dirty_managed_paths");
}

#[test]
fn check_at_tag_supports_mix_projects() {
    let directory = TempDir::new().unwrap();
    fs::write(
        directory.path().join("mix.exs"),
        "defmodule Decant.MixProject do\n  use Mix.Project\n\n  def project do\n    [app: :decant, version: \"2026.9.1\"]\n  end\nend\n",
    )
    .unwrap();
    fs::write(
        directory.path().join(".chrono.toml"),
        "schema = 1\nversion_source = 'mix.exs' # single quotes\n",
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
    git(directory.path(), &["tag", "v2026.9.0"]);
    git(directory.path(), &["tag", "v2026.9.1"]);

    let value = json(&check_at_tag(directory.path()));
    assert_eq!(
        value["error"]["message"],
        "HEAD tags (v2026.9.1,v2026.9.0) do not match mix.exs version 2026.9.1 \
         (expected only v2026.9.1)"
    );
}

#[test]
fn check_prefix_requires_at_tag() {
    let project = fixture();
    let output = run(&["check", "--prefix", "chrono-v"], project.path());
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn status_lists_tags_at_head() {
    let project = fixture();
    git(project.path(), &["tag", "v2026.7.0"]);
    git(
        project.path(),
        &["commit", "--quiet", "--allow-empty", "-m", "next"],
    );
    git(project.path(), &["tag", "v2026.8.0-rc"]);
    git(project.path(), &["tag", "unrelated"]);
    let output = run(
        &["--format", "json", "status", "--date", "2026-08-27"],
        project.path(),
    );
    assert!(output.status.success());
    let value = json(&output);
    let names = value["tags_at_head"]
        .as_array()
        .unwrap()
        .iter()
        .map(|tag| tag["name"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(names, ["v2026.8.0-rc"]);
    assert_eq!(value["tags_at_head"][0]["annotated"], false);
}
