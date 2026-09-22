# ADR 0001: Application-first calendar versioning

Status: accepted  
Date: 2026-08-28

## Decision

ChronoStamp is release identity and automation for applications, services, firmware, and build artifacts. It is not a compatibility-version replacement for public dependencies.

Public libraries must use the compatibility scheme understood by their package manager, normally SemVer. ChronoStamp identity may be carried separately in application artifacts, release metadata, or tags.

The current combined Rust package remains ChronoStamp-versioned while it is an unpublished preview and therefore declares `publish = false`. After package separation, the reusable core will be published with SemVer. The CLI and application artifacts may retain ChronoStamp release identity.

## Rationale

ChronoStamp deliberately defines ordering that differs from SemVer. A calendar year in the SemVer major position also does not communicate API compatibility: a range may admit later months in one year and reject a compatible release in the next year.

Keeping the preview unpublished permits dogfooding without presenting its calendar version as a public library compatibility promise.

## Consequences

- Documentation must state where ChronoStamp is inappropriate.
- Core and CLI package versions may diverge after separation.
- ChronoStamp cannot manage the published core's SemVer package version without a future dual-version adapter.
- The preview package must not be published accidentally.
