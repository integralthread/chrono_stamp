# Contracts and support

## Product contract

ChronoStamp identifies calendar-anchored application releases and artifacts. It does not encode dependency compatibility, and its ordering must not be assumed to match a package manager's SemVer implementation.

## Authoritative versions

Release calculation uses the configured primary version source plus every valid Git tag under the configured tag prefix. The prefix is strict: an unparseable matching tag is a warning for read-only commands and a preflight error for mutating commands. Repositories with legacy tags should configure a distinct prefix such as `chrono-v`.

Future authoritative versions follow the same read/write split. Without `--allow-future`, read commands return available state with a warning and no next candidate, while mutating commands fail. With `--allow-future`, the later authoritative month becomes the effective month and calculation remains monotonic.

## JSON contract

Schema 2 applies to every successful and failed JSON envelope. Every envelope contains `schema_version: 2`, `ok`, and `warnings`; warning-free results use an empty array. Successful `status` output may contain `next: null` only when no safe candidate is available.

Argument failures exit 2. Project, version, file, and Git failures exit 1. Read-only warnings do not change a successful exit status.

## Mutation and recovery guarantees

- Dry-run and real execution share deterministic preflight.
- Every configured target is validated before writes begin.
- Planned files are byte-compared immediately before staging.
- Each replacement is installed through a sibling temporary and backup.
- Cross-file rollback is attempted but is not crash-atomic.
- Git commit, tag, and push are separately durable phases.
- Release errors identify durable state and recovery actions.

## Support matrix

| Project type | Read | Update | Release | Status |
|---|---|---|---|---|
| Standalone Cargo package | Supported | Supported with lockfile validation | Supported local workflow | Preview |
| Cargo workspace/inherited version | Supported | Supported for one shared version owner | Supported local workflow | Preview |
| package.json | Supported | Supported with formatting caveat | No lockfile integration | Preview |
| Mix project | Heuristic read/write | Heuristic | Not recommended | Preview |
| Explicit generic text source | Supported | Supported | Local workflow only | Preview |

Implicit discovery is bounded by the containing Git worktree. Outside Git, only the starting directory is inspected unless `--project` or `--config` is supplied.
