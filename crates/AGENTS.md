# Mova Crates AGENTS

These instructions apply to Rust crates under `crates`. Repository-wide rules live in the root `AGENTS.md`; this file defines application, persistence, domain, and scanning ownership.

## Ownership

- `crates/mova-application`: application-layer business logic
- `crates/mova-db`: SQL, persistence, and synchronization
- `crates/mova-domain`: shared domain models
- `crates/mova-scan`: media discovery, parsing, probing, and sidecars

Keep SQL in `mova-db`, orchestration in `mova-application`, shared meaning in `mova-domain`, and file-analysis behavior in `mova-scan`. Follow `migrations/AGENTS.md` when crate work requires a schema change.

## Scanning behavior

- A newly created library triggers its initial scan automatically.
- Explicit `Scan Library` operations reconcile added, renamed, moved, and deleted files; there is no library watcher.

## Verification

- Run targeted checks and tests such as `cargo check -p ...` and `cargo test -p ...`.
- Verify both frontend and backend when a contract or user-visible workflow spans them.

## Documentation

- Update `crates/README.md` or the relevant crate README when module ownership, runtime behavior, or operating instructions change.
- Update `docs/API.md` when application behavior changes an API contract.
