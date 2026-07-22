# Mova Server AGENTS

These instructions apply to the Rust HTTP service under `apps/mova-server`. Repository-wide rules live in the root `AGENTS.md`; this file only defines service-entry and routing responsibilities.

## Responsibilities

- Own HTTP serving, `/api` routes, handlers, bootstrap, and runtime integration.
- Keep business logic out of handlers and place it in `crates/mova-application`.
- Keep SQL and persistence out of the service entry point and place them in `crates/mova-db`.
- A newly created media library triggers its initial scan automatically.
- File additions, renames, moves, and deletions are reconciled by the explicit `Scan Library` operation; there is no library watcher.

## Verification

- Run targeted Rust checks for service changes, such as `cargo check -p mova-server`.
- When a change affects application logic, persistence, migrations, or scanning, follow the applicable directory-level `AGENTS.md` and run its checks too.

## Documentation

- Update `apps/mova-server/README.md` when server startup, routing, runtime behavior, or deployment integration changes.
- Update `docs/API.md` when an API route, request, response, field, error, or behavior changes.
