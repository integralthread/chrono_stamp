# ChronoStamp Improvement Plan

Status: committed slices implemented locally; remote CI confirmation pending

Prepared: 2026-08-27

Revised: 2026-08-28

Scope: Rust `chrono_stamp` library and `chrono` CLI

## 1. Outcome

Move ChronoStamp from a strong local prototype to a narrowly positioned, dependable release tool without turning a roughly 3,200-line codebase into an open-ended platform project.

The accepted v1 position is:

> ChronoStamp is calendar-anchored release identity and automation for applications, services, firmware, and build artifacts. It is not a substitute for semantic compatibility versioning in public dependency ecosystems.

The immediate commitment is two implementation slices:

1. prevent commands from silently selecting, calculating, or describing the wrong release;
2. make Cargo workspace and lockfile releases internally complete.

A minimal cross-platform CI gate must be in place before the second slice. Work beyond those slices is a candidate roadmap and must be re-triaged from evidence after both slices land.

### Implementation progress — 2026-08-28

- Slice 1 is implemented with regression coverage for bounded discovery, monotonic explicit bumps, strict and normalized tag handling, future-version warnings, predictive initialization, dirty managed paths, and JSON schema 2.
- The Linux/macOS/Windows CI workflow is present. Local equivalents pass; the first hosted matrix run remains pending.
- Slice 2 is implemented for standalone Cargo packages, virtual workspaces, and members inheriting `workspace.package.version`. Manifest and lockfile changes are planned together, Cargo refreshes and validates lock state, and failures attempt restoration before returning.
- Generic `init --initial-version` and schema-1 no-op output-field removal were pulled forward from the candidate backlog because their decisions and implementation boundaries were already settled.
- The next required action is the re-triage checkpoint in Section 9 after hosted CI results are available.

## 2. Current baseline to preserve

The review confirmed that the current implementation already has a solid safety baseline:

- `bump`, `tag`, and `release` dry runs are inert in the covered workflows;
- inconsistent secondary targets stop `bump` before any write;
- update plans re-read and byte-compare files immediately before installation;
- updates use sibling temporary files, preserve permissions, and attempt backup-based rollback;
- configuration rejects unknown keys, duplicate literal targets, unsafe relative paths, and invalid schemas;
- update-time path validation rejects symlinks and targets outside the canonical project root;
- Git is invoked with argument arrays and an explicit working directory, never through a shell;
- CLI usage failures return exit 2, application failures return exit 1, and JSON errors have stable top-level codes;
- release failures already identify the failed phase and provide human recovery text;
- the current test, formatting, documentation, and pedantic Clippy checks are green.

All implementation work must preserve these properties. The plan extends them where the review found gaps; it does not replace them wholesale.

## 3. Confirmed risks and priority

The following behaviors were verified against the current implementation.

| Priority | Risk | Current consequence | Planned response |
|---|---|---|---|
| P0 | Discovery crosses a repository boundary | A command can adopt and mutate an unrelated project above the current repository | Stop discovery and workspace lookup at a defined boundary |
| P0 | `bump --increment` bypasses monotonicity | A correct-looking command can silently lower a version | Compare explicit increments with all authoritative versions |
| P0 | Normalized tag aliases are not detected | Hand-created aliases can represent the same logical release twice | Index tags by parsed `ChronoStamp` identity |
| P0 decision | This library versions itself with ChronoStamp | The repository contradicts its accepted application-first position | Decide the published core/CLI version model in Slice 1 |
| P1 | Malformed matching-prefix tags disappear | Read and mutation decisions omit relevant repository state | Warn on reads; fail mutations |
| P1 | Future tags make `status` fail | The primary inspection command becomes unavailable | Return state with a warning and unavailable `next` |
| P1 | `init --dry-run` skips real preflight | Preview can succeed where the real command fails | Share the same preflight path |
| P1 | Workspace versions are readable but not writable | `status` promises a next version that `bump` cannot deliver | Track and update the actual version owner |
| P1 | Cargo lock state is omitted | A release commit can leave `Cargo.lock` stale | Regenerate, validate, and commit it |
| P1 | Generic `init` references a missing `VERSION` | Initialization creates an unusable project | Require `--initial-version` and create both files |
| P1 | JSON updates reformat the whole document | Version changes create unrelated diffs | Use a format-preserving edit |
| P1 | Mix reading and writing use different heuristics | The same file can be read one way and rejected on write | Mark preview or implement a real adapter |
| P2 | Backup cleanup can turn success into failure | All targets are updated but the command exits 1 | Report cleanup failures as warnings with paths |
| P2 | Release phase is not structured JSON data | Automation must parse prose | Add first-class `phase` and recovery fields |
| P2 | Some CLI/config surface is misleading or inert | Users can set options that have no effect | Remove or finish the surface |
| P3 | Raw argv JSON detection is position-blind | A future argument shape could trigger the wrong envelope | Pin and then replace with parser-aware behavior |

The workspace defect currently fails closed with a confusing error; it is an availability and contract problem, not a demonstrated data-corruption path. Logical duplicate tags remain P0 because release decisions consume tags that may have been created outside ChronoStamp.

## 4. Decisions

### 4.1 Accepted

These questions are resolved and should not be reopened during implementation without new evidence.

1. **Product position:** application-first CalVer is accepted. Public dependency libraries should use a compatibility scheme understood by their package manager.
2. **Malformed and future tags:** read-only commands warn and continue where possible; mutating commands fail preflight.
3. **Generic initialization:** when no supported version source exists, `init` requires `--initial-version VERSION` and creates `VERSION` plus `.chrono.toml` through one planned operation.
4. **Unused output configuration:** remove `output.json_path` and `output.artifact_path` from schema 1. Names may remain reserved in prose for a later schema.
5. **CI sequencing:** a minimal Linux/macOS/Windows matrix is required before workspace, path, and transaction architecture changes begin.
6. **Transaction scope:** do not build a journal or `chrono recover` command now. Keep collision-resistant temporary names and structured rollback reporting; rely on Git recovery for clean-tree release flows.
7. **The project’s version model:** keep the combined preview crate on ChronoStamp while setting `publish = false`. After separation, publish the reusable core with SemVer and version the CLI/application artifacts with ChronoStamp. The positioning ADR must describe this preview dogfooding arrangement and its transition.
8. **Non-Git discovery:** outside a Git worktree, implicit discovery inspects only the starting directory. Searching from a nested non-Git directory requires explicit `--project` or `--config`.
9. **Future-version override:** `--allow-future` never authorizes a backward candidate. When supplied, calculation uses the later of the requested/current month and the latest authoritative month, then applies normal monotonic progression.
10. **JSON compatibility:** introduce `schema_version: 2` with the tag/future warning work. Every JSON envelope includes `warnings`, using an empty array when there are none. A successful `status` may return `next: null` only when no safe candidate is available.
11. **Tag-prefix migration:** the configured prefix is strict. Every unparseable tag under that prefix warns on reads and blocks mutations. Repositories with legacy SemVer tags must select a distinct prefix, such as `chrono-v`; no ignore escape hatch is added initially.

### 4.2 Deferred decisions

These do not block Slice 1:

- whether a non-atomic push escape hatch is operationally necessary;
- which JavaScript package managers enter the supported release matrix;
- whether Mix remains preview-only or receives a first-class adapter;
- whether Serde is a default or optional core dependency;
- core and CLI MSRVs after separation;
- target platforms and registries for preview distribution.

Each deferred decision must be resolved only when its associated candidate work is accepted. None blocks Slice 1 or Slice 2.

## 5. Delivery model

| Stage | Commitment | Purpose |
|---|---|---|
| Slice 1 | Committed | Close silent wrong-project and wrong-version behavior |
| CI gate | Committed | Protect path, rename, newline, and Git behavior across platforms |
| Slice 2 | Committed | Make Cargo workspace releases complete |
| Re-triage checkpoint | Required | Review evidence, scope, and product fit before further architecture |
| Candidate roadmap | Not yet committed | Hardening, separation, broader ecosystems, and distribution |

Each slice may be delivered as several small pull requests. A slice is complete only when all of its acceptance criteria pass.

## 6. Slice 1 — release-decision safety

Priority: P0  
Goal: a correct-looking command must not silently act on the wrong project or move release identity backward.

### 6.1 Contract and positioning update

Create one short positioning ADR and one combined contracts/support document. Avoid a documentation program larger than the implementation.

The documents must:

- state the accepted application-first position;
- explain that ChronoStamp ordering is not SemVer precedence;
- show one Cargo/npm range example demonstrating the compatibility mismatch;
- define authoritative versions as the primary source plus valid configured-prefix Git tags;
- define read-versus-mutate treatment of malformed and future tags;
- state the exact file and Git recovery guarantees;
- mark Cargo standalone, Cargo workspace, package.json, Mix, and generic text support as supported, preview, or unsupported for read/update/release;
- document the approved preview dogfooding and post-split package-version model.

Deliverables:

- `docs/adr/0001-product-position.md`;
- `docs/CONTRACTS_AND_SUPPORT.md`;
- focused updates to `README.md`, `docs/SPEC.md`, and `docs/CONFIGURATION.md`.

Generated-reference drift automation is deferred to the CI/distribution candidate work.

### 6.2 Stop discovery at the project boundary

Implement one boundary calculation shared by project discovery and Cargo workspace lookup.

Required behavior:

- explicit `--project` and `--config` remain authoritative;
- implicit discovery may inspect the start directory and its ancestors only through the nearest repository boundary;
- discovery must not walk above the first containing Git worktree;
- nested directories inside a worktree may find project configuration or a manifest inside that worktree;
- Cargo workspace inheritance must stop at the same boundary;
- unreadable or invalid candidate workspace manifests must produce a diagnostic rather than being skipped in favor of an older ancestor;
- outside Git, implicit discovery inspects only the starting directory; nested use requires explicit `--project` or `--config`;
- mutating command output includes the selected project root and version source in verbose/JSON preflight data.

Design note:

Use Git’s reported worktree root when available rather than assuming that a `.git` entry is always a directory. This covers worktrees and submodules. Provide a filesystem-only fallback when Git is unavailable or the start path is not in a repository.

Tests:

- nested directory finds the intended project;
- repository subdirectory with no manifest does not adopt a parent repository’s project;
- linked worktree and `.git` file boundary;
- nested repositories;
- explicit project/config outside the implicit boundary;
- workspace root and member lookup;
- malformed nearer workspace manifest does not silently fall through;
- no-Git filesystem discovery.

Acceptance criterion:

- no implicit command can select a project outside its discovered boundary.

### 6.3 Enforce monotonic explicit bumps

The current non-monotonic surface is `bump --increment`; `release` does not have an increment option.

Implement explicit-increment policy inside the pure next-version calculation:

- compare against the highest authoritative release in the target month;
- allow an explicit increment greater than the highest increment;
- allow a valid promotion at the same increment only when the requested stability is greater;
- reject equal, lower, repeated, or less-stable candidates;
- reject future authoritative versions for mutating commands unless `--allow-future` is supplied;
- when `--allow-future` is supplied, use the later of the requested/current month and latest authoritative month, then require the resulting candidate to remain monotonic;
- do not add an override flag in this slice;
- return an error containing the requested candidate and blocking authoritative version.

Tests:

- lower, equal, and greater explicit increments;
- promotion and demotion at the same increment;
- collision present only in Git tags;
- no-release and new-month behavior;
- future versions;
- `u64::MAX`.

Acceptance criterion:

- every successful explicit bump is a strict progression or a documented same-increment promotion.

### 6.4 Build a complete tag catalog

Replace silent parsing/filtering with a catalog containing:

- valid configured-prefix tags;
- malformed configured-prefix tags and parse errors;
- unrelated tags;
- normalized version-to-literal-tag mappings.

Command policy:

- `status` and `history` return valid state plus structured warnings;
- `bump`, `tag`, and `release` fail before mutation on malformed relevant tags;
- the configured prefix is strict: every unparseable matching tag is relevant;
- migration from legacy tags requires a distinct configured prefix, with `chrono-v` documented as an example;
- `tag` and `release` reject any literal tag whose parsed version equals the candidate, including display/canonical aliases;
- duplicate normalized tags already present appear as warnings on reads and errors on mutations;
- human `--quiet` may suppress read warnings, but never mutation failures;
- all JSON envelopes use `schema_version: 2` and contain a stable `warnings` array, including failures and warning-free results.

Tests:

- `v2026.8` and `v2026.8.0` identity collision;
- `v2026.8.0-final` alias;
- malformed matching-prefix tags;
- tags outside the prefix;
- prefix overrides;
- stable history output with duplicate normalized versions.

Acceptance criterion:

- no tag relevant to a release decision disappears silently.

### 6.5 Make read-only future-version handling graceful

When an authoritative version is later than the supplied/current UTC month and `--allow-future` is absent:

- `status` still reports project, source version, Git state, and tags;
- `status.next` is `null`/unavailable and a structured warning explains why;
- `history` continues to list the tag and may annotate it;
- `bump` and `release` fail unless their existing explicit future policy allows the operation;
- warnings use stable codes shared with malformed-tag reporting.

With `--allow-future`, read and mutating commands use the latest authoritative month as the effective month when it is later, calculate a monotonic candidate from that month, and make the effective-month choice explicit in human and JSON output.

The schema-2 migration must be applied consistently to usage errors, application errors, and every successful command. Schema 1 is not redefined in place.

Tests must cover human, quiet, and JSON output at fixed dates.

### 6.6 Make `init` dry-run predictive

Refactor `init` so dry-run and real execution share destination resolution and preflight.

Both paths must identically detect:

- an existing configuration without `--force`;
- a configuration symlink;
- an invalid or inaccessible project path;
- missing parent-directory permissions where that can be checked without mutation;
- unsupported generic-project arguments.

Dry-run must stop after planning, before opening or writing files.

Also correct project-path canonicalization errors so they refer to the project path, not the current directory.

Acceptance criterion:

- every deterministic preflight failure has the same status and error code in dry-run and real mode.

### 6.7 Protect recoverable working state

For `bump` and `sync` inside Git:

- inspect managed paths before mutation;
- fail when a managed path has uncommitted changes unless the user explicitly supplies a narrowly named override;
- do not require the entire worktree to be clean;
- show the affected managed paths in the error;
- retain existing all-worktree cleanliness requirements for `tag` and `release`.

This is a proportionate substitute for a general transaction journal: Git remains the recovery mechanism without blocking unrelated development files.

### 6.8 Slice 1 acceptance gate

Slice 1 is complete when:

- application-first positioning and the project’s own version model are consistent;
- the current combined package is explicitly non-publishable, with the core SemVer/CLI ChronoStamp split documented;
- implicit discovery cannot cross its boundary;
- `bump --increment` cannot move backward or repeat a release;
- read commands expose malformed, duplicate, and future tags without becoming unusable;
- mutating commands fail on the same conditions before mutation;
- normalized tag aliases cannot be created by `tag` or `release`;
- `init --dry-run` predicts real deterministic preflight failures;
- dirty managed paths are protected;
- human and JSON contracts are documented and tested;
- every JSON path emits schema 2 with `warnings`, and `status.next` is explicitly nullable;
- the existing safety baseline remains green.

## 7. CI gate — before Slice 2

Priority: P0 sequencing requirement

Add a minimal pull-request matrix before changing workspace ownership or lockfile behavior:

- Linux stable: format check, Clippy with warnings denied, tests, docs;
- macOS stable: tests;
- Windows stable: tests;
- one job that verifies the emitted usage specification still parses;
- deterministic Git fixture configuration for name, email, default branch, locale, and hooks.

Portability requirements:

- test CRLF and Windows paths;
- gate Unix permission assertions appropriately;
- avoid dependence on global Git configuration, user hooks, timezone, pager, editor, signing setup, or default shell;
- keep all date-sensitive tests fixed;
- make temporary-file and rename tests run on all supported platforms.

This gate is intentionally smaller than the eventual distribution matrix. MSRV, package-content checks, dependency policy, generated references, and release artifacts remain candidate work.

Acceptance criterion:

- Slice 1 is green on Linux, macOS, and Windows before Slice 2 merges begin.

## 8. Slice 2 — Cargo workspace and lockfile correctness

Priority: P0 contract completion

Goal: if ChronoStamp reports that a Cargo release is possible, it must update the actual version owner and leave tracked Cargo version state coherent.

### 8.1 Introduce a version-source model

Replace the current independent read/write kind decisions with a resolved source containing:

- ecosystem kind;
- logical package identity;
- declaration manifest;
- actual version-owner manifest and key;
- project and repository roots;
- generated files;
- supported operations;
- validation adapter.

For Cargo, represent separately:

- standalone `[package].version`;
- workspace `[workspace.package].version`;
- member `version.workspace = true`;
- multiple members sharing one version owner;
- virtual manifests without `[package]`.

The same resolved source must drive `status`, `check`, `bump`, and `release`. A read-only command must not advertise a write path that the resolver knows is unsupported.

### 8.2 Resolve Cargo workspace ownership

Required behavior:

- discovery from a workspace root and nested member;
- update the owner’s `workspace.package.version` for inherited members;
- preserve member inheritance declarations;
- reject ambiguous implicit selection when independent package versions exist;
- permit explicit package/project selection;
- keep project-root and repository-root path rules distinct;
- report unsupported workspace shapes before calculating a candidate version.

The verified current failure is expected to remain fail-closed until this work lands. Improve its diagnostic immediately if it is encountered during Slice 1.

### 8.3 Regenerate and validate `Cargo.lock`

Use Cargo as the authority rather than directly guessing all lockfile edits.

The Cargo adapter must:

- detect whether `Cargo.lock` exists and whether it is tracked;
- snapshot relevant pre-operation bytes;
- update manifest candidates;
- regenerate lock state using a documented Cargo command in a controlled phase;
- verify that lockfile changes correspond to the selected workspace/package release;
- include the lockfile in the update plan and release commit;
- run a locked validation after regeneration;
- restore manifest and lockfile bytes if regeneration or validation fails before commit;
- report when no lockfile is expected for the selected project policy.

Dry-run contract:

- perform all non-mutating resolution and preflight;
- report that lockfile regeneration is planned;
- do not claim exact lockfile bytes unless they were derived without changing the real worktree;
- never run a mutating Cargo command against the real project during dry-run.

### 8.4 Keep updates scoped

Before staging a release:

- compare the changed path set with the resolved manifest, lockfile, and configured targets;
- reject unexpected generated changes;
- stage only approved paths;
- after commit, verify the commit contains exactly the intended paths;
- re-read the authoritative version from the committed tree before tagging.

### 8.5 Slice 2 tests

Required fixtures:

- standalone Cargo package with and without tracked `Cargo.lock`;
- virtual workspace;
- one inherited-version member;
- several members sharing a workspace version;
- independent member versions requiring explicit selection;
- nested invocation from a member directory;
- stale lockfile;
- Cargo regeneration failure;
- unexpected generated-file change;
- dry-run on every fixture;
- post-operation `cargo check --locked` and clean-worktree assertion.

### 8.6 Slice 2 acceptance gate

Slice 2 is complete when:

- this repository can release without leaving its tracked `Cargo.lock` stale;
- inherited workspace versions can be read and updated through the same resolved owner;
- unsupported or ambiguous workspace shapes fail before mutation with useful diagnostics;
- dry-run describes the Cargo phases without modifying the worktree;
- a successful release commit contains exactly the expected manifest, lockfile, and configured targets;
- `cargo check --locked` succeeds and the worktree remains clean;
- the cross-platform CI gate remains green.

## 9. Re-triage checkpoint

After Slice 2, stop and review:

- defect reports and complexity added by Slices 1 and 2;
- whether application-first positioning matches actual adopters;
- whether the core library is being consumed independently;
- which non-Cargo ecosystems have real users;
- whether Git release/push automation is used in production;
- whether rollback limitations have caused real recovery problems;
- dependency size and MSRV pressure;
- CLI JSON consumers and schema stability needs.

Only then promote candidate work into committed milestones. Prefer small fixes over speculative subsystems.

Required outputs:

- updated support matrix;
- closed/open P0 list;
- a short decision record selecting the next one or two slices;
- revised estimates based on actual implementation experience.

## 10. Candidate roadmap after re-triage

This section is ordered but not committed.

### 10.1 Update and configuration correctness

Likely next priority:

- implement accepted generic `init --initial-version` behavior and create `VERSION` plus config through one plan;
- replace whole-document JSON serialization with a format-preserving string-token edit;
- test compact JSON, indentation, CRLF, key order, escaped text, Unicode, duplicate keys, and trailing-newline preservation;
- replace optional update-target fields with tagged internal variants;
- canonicalize target identity before duplicate checks;
- remove `output.json_path` and `output.artifact_path` from schema 1;
- decide Mix support: mark it explicit preview with warnings or build one parser/updater adapter shared by reads and writes.

Exit criteria:

- initialization never produces an unusable project;
- JSON version changes have minimal diffs;
- schema 1 has no accepted no-op fields;
- read and write behavior agree for every advertised ecosystem.

### 10.2 Git release hardening

Candidate work:

- validate HEAD, branch/upstream policy, committer identity, remote existence, signing prerequisites, and managed path tracking before mutation;
- reject detached HEAD by default or require an explicit destination ref in CI;
- use explicit branch/tag refspecs;
- use `git push --atomic` by default;
- initially provide no non-atomic escape hatch;
- after commit and hooks, verify HEAD, intended path set, authoritative version, and worktree state before tagging;
- keep hook behavior visible and never bypass hooks implicitly.

Structured output must extend the existing release-phase mechanism rather than replace it:

- add `phase`, `recovery`, durable commit/tag identifiers, and affected paths as first-class JSON fields;
- preserve useful human recovery text;
- distinguish preflight, planning, regeneration, installation, staging, commit, tag, and push.

Exit criteria:

- default push cannot publish only the branch or only the tag;
- hook-created unintended commit content is detected before tagging;
- automation never needs to parse the human message to identify a release phase.

### 10.3 Right-sized transaction hardening

Do now if touched:

- replace PID-only temporary names with collision-resistant names;
- return structured rollback failures rather than discarding them;
- keep at least one recoverable original/backup copy on installation failure;
- report backup-cleanup failures after successful installation as warnings, not command failure;
- include leftover backup paths and safe cleanup guidance.

Defer unless non-Git use produces evidence:

- transaction journals;
- startup scanning for abandoned operations;
- a `chrono recover` command;
- crash-recovery orchestration beyond preserving recoverable files.

### 10.4 Core and CLI separation

Candidate end state:

```text
workspace/
├── crates/chrono_stamp/       # model, parsing, ordering, next-version policy
└── crates/chrono_stamp_cli/   # config, project adapters, updates, Git, CLI
```

Goals:

- core users do not compile the CLI framework, Git/project, regex, or document-editing stack;
- core and CLI can declare evidence-based MSRVs;
- Serde can be evaluated as a core default or optional feature;
- the published core follows the accepted version decision from Section 4.1;
- the `chrono` executable name is preserved unless collision research justifies a change;
- public APIs exclude accidental CLI JSON stability commitments.

### 10.5 CLI cleanup

Bundle small, verified issues:

- make `--color` affect human diagnostics or remove it;
- define `--quiet` consistently;
- correct misleading project-path diagnostics;
- avoid an extra completion newline;
- add structured warnings to successful JSON;
- replace or pin positional-blind `--format json` detection;
- test broken-pipe behavior.

### 10.6 Distribution and stabilization

After core/CLI boundaries and support scope stabilize:

- test declared MSRVs;
- verify package/archive contents;
- generate manpages, Markdown reference, and completion artifacts;
- publish supported platform binaries with checksums;
- add dependency license/vulnerability policy;
- add `SECURITY.md`, support policy, changelog policy, and signing policy;
- test installation on clean machines;
- collect preview evidence from standalone Cargo and inherited-workspace projects;
- require two consecutive release candidates with no open P0 correctness or data-loss issues.

## 11. Test strategy

### 11.1 Core policy

- grammar acceptance/rejection and normalization aliases;
- display/canonical round trips;
- equality/hash consistency;
- total-order transitivity and antisymmetry;
- documented SemVer-divergence examples;
- monotonic calculated and explicit progression;
- year/month and increment boundaries;
- future-version policy.

### 11.2 Discovery and configuration

- repository and workspace boundaries;
- explicit override precedence;
- worktrees, submodules, nested repositories, and no-Git directories;
- ambiguous ecosystem roots;
- path aliases, traversal, symlinks, and outside-root targets;
- invalid nearer manifests do not fall through;
- dry-run and real preflight parity;
- schema unknown/no-op field behavior.

### 11.3 Updates

- consistency failures before writes;
- concurrent byte changes;
- temporary creation and rename failures;
- rollback failure reporting;
- backup cleanup warning semantics;
- permissions, CRLF, newline, and JSON formatting preservation;
- dirty managed-path protection;
- exact changed-path sets.

### 11.4 Git

- malformed, future, duplicate-name, and duplicate-normalized tags;
- dirty index/worktree and untracked managed files;
- unborn and detached HEAD;
- hook failure and post-commit mutation;
- signing and remote preflight through controlled adapters;
- atomic push success and unsupported-remote failure;
- structured phase/recovery JSON.

### 11.5 CLI contracts

- stdout/stderr separation;
- exit 0/1/2 categories;
- exactly one JSON object on JSON paths;
- stable error and warning codes;
- `next: null` with future-version warning;
- quiet/verbose/color behavior;
- dry-run mutation audit;
- help, completion, and usage-spec round trips;
- positional `--format json` edge cases;
- broken pipes.

## 12. Work register

### 12.1 Committed

| ID | Work item | Priority | Size | Depends on |
|---|---|---:|---:|---|
| CST-001 | Apply approved positioning, preview publishing, and version contracts | P0 | M | — |
| CST-002 | Bound project and workspace discovery | P0 | M | — |
| CST-003 | Enforce monotonic explicit bumps | P0 | M | CST-001 |
| CST-004 | Catalog malformed and normalized-duplicate tags | P0 | M | CST-003 |
| CST-005 | Graceful future-tag behavior on reads | P1 | S | CST-004 |
| CST-006 | Make `init` dry-run predictive and fix path diagnostics | P1 | S | — |
| CST-007 | Protect dirty managed paths for `bump`/`sync` | P1 | S | CST-002 |
| CST-008 | Add minimal cross-platform CI gate | P0 | M | CST-002–CST-007 |
| CST-009 | Introduce resolved version-source model | P0 | L | CST-008 |
| CST-010 | Support Cargo workspace version owners | P0 | L | CST-009 |
| CST-011 | Regenerate and commit Cargo lock state | P0 | L | CST-009, CST-010 |

### 12.2 Candidate after re-triage

| ID | Work item | Priority now | Size | Depends on |
|---|---|---:|---:|---|
| CST-012 | Implement generic `init --initial-version` transaction | P1 | M | CST-009 |
| CST-013 | Preserve JSON formatting | P1 | M | CST-009 |
| CST-014 | Use typed update targets and remove no-op schema fields | P1 | M | CST-009 |
| CST-015 | Decide and align Mix read/write support | P1 | M | CST-001, CST-009 |
| CST-016 | Atomic push and expanded Git preflight | P1 | L | CST-011 |
| CST-017 | Post-hook commit verification and structured release JSON | P1 | M | CST-016 |
| CST-018 | Right-sized transaction cleanup/rollback improvements | P2 | M | CST-011 |
| CST-019 | Separate core and CLI packages | P1 | L | CST-001, CST-011 |
| CST-020 | CLI surface cleanup | P2 | M | CST-004 |
| CST-021 | Packaging, MSRV, security, and preview distribution | P1 | L | CST-019 |

Sizes are relative: S is focused, M spans several modules/tests, and L changes workflow or package boundaries.

## 13. Definition of done

Every implementation change must include:

- the invariant being added or changed;
- success, negative, and recovery-path tests proportional to the risk;
- human and JSON output changes where applicable;
- fixed dates and isolated Git configuration in tests;
- documentation and compatibility notes;
- proof that dry-run performs no project mutation;
- platform impact assessment;
- formatting, Clippy, tests, and documentation checks passing.

For mutating workflows, success-path coverage alone is insufficient.

## 14. Next action

Begin with CST-001 and CST-002 in separate reviewable changes:

1. apply `publish = false` to the combined preview crate and document the approved core/CLI version model plus JSON schema 2 contract;
2. implement a shared repository/workspace discovery boundary with regression fixtures.

Then implement CST-003 through CST-007 as focused changes, establish the CI gate, and begin the Cargo source/lockfile slice only after the gate is green.
