# Mova Migrations AGENTS

These instructions apply to database schema files under `migrations`. Repository-wide pre-1.0 database rules live in the root `AGENTS.md`.

## Schema changes

- During pre-1.0 development, modify `migrations/0001_init.sql`; do not add `0002`, `0003`, or compatibility migrations.
- Design tables and fields for the current domain model. Refactor an unsuitable development schema instead of preserving workarounds for old data.
- Update affected Rust queries, response models, TypeScript types, and documentation in the same change.
- State explicitly whether the change requires rebuilding the database, resetting the data directory, or rescanning media.

## Documentation

- Update `docs/API.md` when a schema change affects API requests, responses, or behavior.
- Update relevant README files when initialization, reset, or deployment behavior changes.
