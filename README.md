# ChronoStamp

ChronoStamp is calendar-anchored release identity and automation for applications, services, firmware, and build artifacts. The Rust package contains a strongly typed `chrono_stamp` implementation library and a `chrono` project-management CLI.

A version combines a UTC year and unpadded month with a monthly increment:

```text
2026.8.0-dev → 2026.8.0-alpha → 2026.8.0-beta → 2026.8.0-rc → 2026.8.0
```

Development builds can use a Git hash instead: `2026.8.0-94fdabe`.

## When not to use ChronoStamp

Do not use a ChronoStamp as the compatibility version of a public dependency. Cargo, npm, Hex, and other SemVer-oriented package managers apply their own precedence and range rules rather than ChronoStamp ordering. For example, a Cargo requirement such as `^2026.8.0` can accept later `2026.x` months while excluding `2027.1.0`; neither result says anything reliable about API compatibility.

Use SemVer for a published library's package version. Use ChronoStamp for application releases and artifact identity where the calendar anchor is the intended meaning.

The current combined Rust package is an unpublished preview that dogfoods ChronoStamp. The planned package split will publish the reusable core with SemVer while retaining ChronoStamp identity for the CLI and application artifacts.

## Install and try it

```sh
mise install
mise exec -- cargo build
mise exec -- cargo run -- parse 2026.8-rc --canonical
cargo install --path .
```

The installed executable is named `chrono`. Run `chrono --help` or `chrono COMMAND --help` for the generated command reference.

## Typical project workflow

```sh
chrono init
chrono status
chrono check
chrono bump --tag rc --dry-run
chrono release --tag final --dry-run
chrono release --tag final
```

`init` creates declarative `.chrono.toml` configuration; it does not install hooks. `bump`, `sync`, and `dev --write` validate every configured target before writing. `tag` and `release` require a clean Git worktree. Every mutating workflow has a dry-run path that performs no file or Git mutation.

For CI and scripts, use `--format json`. Successful and failed results carry `schema_version: 2` and a `warnings` array; argument failures exit with status 2, and project, file, or Git failures exit with status 1.

## Commands

- `status` (`version` alias): current source version, highest matching Git tag, commit, branch, dirty state, and next candidate.
- `parse VERSION` and `compare LEFT RIGHT`: validation, normalization, and ordering.
- `dev`: print a UTC-month Git build; `--write` opts into file changes.
- `check`, `history`: consistency and version-tag history.
- `init`, `bump`, `sync`: configuration and safe file updates.
- `tag`, `release`: annotated/signed tags and the preflight-update-commit-tag-push workflow.
- `completion SHELL`: Bash, Zsh, Fish, Elvish, Nushell, or PowerShell completion code.

The CLI is declared with exactly pinned `usage-rs` 6.4.1. Its hidden `__usage_spec__` endpoint emits the portable KDL command contract.

## Configuration and compatibility

`.chrono.toml` supports Cargo/TOML, JSON, and explicitly counted regex update targets. Relative paths are rooted at the project, parent traversal is rejected, and symlink targets are refused. See [configuration](docs/CONFIGURATION.md) for the schema and examples.

## Library

```rust
use chrono_stamp::{ChronoStamp, Stability};

let version: ChronoStamp = "2026.8-rc".parse()?;
assert_eq!(version.to_string(), "2026.8.0-rc");

let final_release = ChronoStamp::release(2026, 8, 0, Stability::Final)?;
assert!(version < final_release);
# Ok::<(), Box<dyn std::error::Error>>(())
```

ChronoStamp ordering is not ordinary SemVer precedence: within a month, Git-hash builds sort below numbered releases, then increments and stability determine release order.

## Development

```sh
mise run fmt-check
mise run lint
mise run test
```

The repository is MIT licensed.
