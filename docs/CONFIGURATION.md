# Configuration

ChronoStamp searches upward for `.chrono.toml`. Without one, it discovers `Cargo.toml`, `mix.exs`, or `package.json` and uses that file as the primary version source. `--project PATH` fixes the project root; `--config PATH` selects configuration directly.

## Schema 1

```toml
schema = 1
version_source = "Cargo.toml"

[git]
tag_prefix = "v"
remote = "origin"
sign_tags = false

[dev]
hash_length = 7

[[updates]]
path = "package.json"
kind = "json"
key = "version"

[[updates]]
path = "README.md"
kind = "regex"
pattern = '(?m)(?<prefix>Version:\s*)[^\s]+(?<suffix>\s|$)'
expected_matches = 1
```

Unknown keys, unsupported schemas, duplicate targets, zero expected matches, and incomplete target definitions are errors. The primary version source is always updated by `bump`, `dev --write`, and `release`; it does not need to appear in `updates`. `sync` changes only secondary targets.

## Target kinds

- `toml` uses a dot-separated string key and preserves the surrounding TOML document through `toml_edit`.
- `json` uses a dot-separated string key and writes pretty, valid JSON.
- `regex` requires an exact `expected_matches`. The replaceable version must be a named `version` capture, or the text between ordered named `prefix` and `suffix` captures.

Before a bump, every target must contain the current primary version. A mismatch stops the operation before any file changes. `sync` deliberately permits mismatches and makes secondary targets agree with the primary.

Configured paths must be relative and cannot contain parent traversal. Resolved targets must remain inside the canonical project root. Symlink targets are refused. Updates are planned in memory, checked again for concurrent changes, written to sibling temporary files with preserved permissions, and installed with backup-based rollback attempts.

The names `output.json_path` and `output.artifact_path` are reserved for a future schema. Schema 1 rejects them rather than accepting no-op configuration.
