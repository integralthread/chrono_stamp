# ChronoStamp format specification

## Grammar

```text
version   = year "." month [ "." increment ] [ "-" suffix ]
year      = four decimal digits, 0001 through 9999
month     = unpadded decimal 1 through 12
increment = unsigned 64-bit decimal integer
suffix    = "dev" / "alpha" / "beta" / "rc" / "final" / git-hash
git-hash  = 7 through 16 lowercase hexadecimal characters
```

An omitted increment means `0`. An omitted suffix means `final`. A Git-hash suffix requires an explicit `.0`; a nonzero hash increment is invalid.

| Input | Display | Canonical |
|---|---|---|
| `2026.8` | `2026.8.0` | `2026.8.0-final` |
| `2026.8.2` | `2026.8.2` | `2026.8.2-final` |
| `2026.8-rc` | `2026.8.0-rc` | `2026.8.0-rc` |
| `2026.8.0-94fdabe` | `2026.8.0-94fdabe` | `2026.8.0-94fdabe` |

Rejected examples include year `0000`, padded month `08`, uppercase or short hashes, unknown stability tags, and `2026.8.1-94fdabe`.

## Ordering

Versions compare by year, month, kind, kind-specific value, then stability. Git builds sort below numbered releases. Releases compare by increment, then `dev < alpha < beta < rc < final`. This is a total ChronoStamp ordering, not SemVer build-metadata precedence.

ChronoStamp ordering is an application-level identity contract. Package managers do not use it when resolving dependency ranges. Published libraries should expose a SemVer package version for compatibility and keep ChronoStamp identity separate.

## Next-version policy

- A new UTC month starts at increment `0`.
- Git builds do not consume a release increment.
- Promoting a prerelease to a more stable tag keeps its increment.
- Repeating a tag or moving to a less stable tag increments.
- A future-dated authoritative version is an error unless explicitly allowed. Allowing it uses the later authoritative month as the effective month and never permits a backward candidate.
- Increment overflow is an error.

All production date calculation uses UTC. The CLI accepts `--date YYYY-MM-DD` on date-sensitive commands for deterministic previews and automation.
