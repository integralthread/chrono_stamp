use std::ffi::OsStr;
use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use chrono_stamp::cli::Cli;
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
        "[package]\nname = \"fixture\"\nversion = \"2026.8.0-rc\"\n",
    )
    .expect("manifest");
    git(directory.path(), &["init", "--quiet"]);
    git(directory.path(), &["config", "user.name", "Chrono Test"]);
    git(
        directory.path(),
        &["config", "user.email", "chrono@example.invalid"],
    );
    git(directory.path(), &["add", "Cargo.toml"]);
    git(directory.path(), &["commit", "--quiet", "-m", "initial"]);
    git(directory.path(), &["tag", "v2026.7.3"]);
    git(directory.path(), &["tag", "v2026.8.0-beta"]);
    directory
}

#[test]
fn usage_spec_round_trips_and_records_strictness() {
    let kdl = Cli::to_kdl();
    let parsed: usage_parser::Spec = kdl.parse().expect("Usage KDL should round-trip");

    assert_eq!(parsed.bin, "chrono");
    assert!(kdl.contains("unknown_flags error"), "{kdl}");
    assert!(kdl.contains("args_override_self #false"), "{kdl}");
    assert!(parsed.cmd.subcommands.contains_key("status"));
    assert!(parsed.cmd.subcommands.contains_key("completion"));
}

#[test]
fn parser_rejects_unknown_and_repeated_scalar_flags() {
    let unknown = [
        OsStr::new("--wat"),
        OsStr::new("parse"),
        OsStr::new("2026.8"),
    ];
    assert!(Cli::parse_from(&unknown).is_err());

    let repeated = [
        OsStr::new("--format"),
        OsStr::new("json"),
        OsStr::new("--format"),
        OsStr::new("human"),
        OsStr::new("parse"),
        OsStr::new("2026.8"),
    ];
    assert!(Cli::parse_from(&repeated).is_err());
}

#[test]
fn parse_and_compare_are_project_independent() {
    let directory = TempDir::new().expect("empty directory");
    let parsed = run(&["parse", "2026.8", "--canonical"], directory.path());
    assert!(parsed.status.success());
    assert_eq!(
        String::from_utf8(parsed.stdout).unwrap(),
        "2026.8.0-final\n"
    );

    let compared = run(
        &["compare", "2026.8.0-abcdef0", "2026.8.0-dev"],
        directory.path(),
    );
    assert!(compared.status.success());
    assert_eq!(String::from_utf8(compared.stdout).unwrap(), "-1\n");
}

#[test]
fn status_discovers_from_a_nested_directory_and_is_deterministic() {
    let project = fixture();
    let nested = project.path().join("one/two");
    fs::create_dir_all(&nested).expect("nested directory");
    let output = run(
        &["--format", "json", "status", "--date", "2026-08-27"],
        &nested,
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).expect("valid JSON");
    assert_eq!(value["schema_version"], 2);
    assert_eq!(value["warnings"], serde_json::json!([]));
    assert_eq!(value["project"]["name"], "fixture");
    assert_eq!(value["version"], "2026.8.0-rc");
    assert_eq!(value["highest_tag"]["version"], "2026.8.0-beta");
    assert_eq!(value["next"], "2026.8.0");
    assert_eq!(value["git"]["dirty"], false);
}

#[test]
fn history_uses_chronostamp_order_and_month_filter() {
    let project = fixture();
    let output = run(
        &["history", "--month", "2026.8", "--limit", "1"],
        project.path(),
    );
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "2026.8.0-beta\tv2026.8.0-beta\n"
    );
}

#[test]
fn parse_failures_have_a_single_json_envelope() {
    let directory = TempDir::new().expect("empty directory");
    let output = run(&["--format", "json", "parse", "2026.08"], directory.path());
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stderr.is_empty());
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).expect("valid JSON");
    assert_eq!(value["schema_version"], 2);
    assert_eq!(value["warnings"], serde_json::json!([]));
    assert_eq!(value["ok"], false);
    assert_eq!(value["error"]["code"], "invalid_value");
}

#[test]
fn status_reports_future_and_invalid_tags_as_warnings() {
    let project = fixture();
    git(project.path(), &["tag", "v2026.9.0"]);
    git(project.path(), &["tag", "vnot-a-chronostamp"]);
    let output = run(
        &["--format", "json", "status", "--date", "2026-08-27"],
        project.path(),
    );
    assert!(output.status.success());
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(value["next"].is_null());
    let codes = value["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|warning| warning["code"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert!(codes.contains(&"future_version"));
    assert!(codes.contains(&"invalid_version_tag"));
}

#[test]
fn allow_future_calculates_from_the_future_month() {
    let project = fixture();
    git(project.path(), &["tag", "v2026.9.2"]);
    let output = run(
        &[
            "--format",
            "json",
            "status",
            "--date",
            "2026-08-27",
            "--allow-future",
        ],
        project.path(),
    );
    assert!(output.status.success());
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["next"], "2026.9.3");
    assert_eq!(value["warnings"][0]["code"], "future_month_adopted");
}

#[test]
fn discovery_does_not_cross_a_repository_boundary() {
    let directory = TempDir::new().unwrap();
    fs::write(
        directory.path().join("Cargo.toml"),
        "[package]\nname = \"wrong\"\nversion = \"2026.8.0\"\n",
    )
    .unwrap();
    let repository = directory.path().join("repository");
    let nested = repository.join("one/two");
    fs::create_dir_all(&nested).unwrap();
    git(&repository, &["init", "--quiet"]);

    let output = run(&["status", "--date", "2026-08-27"], &nested);
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("no ChronoStamp project"));
}

#[test]
fn completion_scripts_are_generated_by_usage() {
    let directory = TempDir::new().expect("empty directory");
    let output = run(&["completion", "zsh"], directory.path());
    assert!(output.status.success());
    let script = String::from_utf8(output.stdout).unwrap();
    assert!(script.contains("chrono"), "{script}");
}
