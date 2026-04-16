---
name: mova-workspace
description: Work inside the Mova self-hosted media server repository. Use when changing Rust backend code, the React/Vite frontend, Docker Compose setup, Postgres-backed media-library features, playback UI, scanning logic, or related docs.
---

# Mova Workspace

Use this skill for work in the current Mova repository.

`AGENTS.md` defines the repo's highest-priority collaboration rules. This skill should focus on execution details and should not restate policy unless needed for clarity.

## Minimal Read Order

- Start with `AGENTS.md`
- Then read `README.md`
- Then read `docs/API.md`
- Then read `docs/ROADMAP.md`
- Read area docs only if needed:
  - Frontend: `apps/mova-web/README.md`
  - Backend: `apps/mova-server/README.md`
  - Crates: `crates/README.md`

## Current Project Truths

- The project is still pre-1.0 and still uses a single migration file: `migrations/0001_init.sql`
- Current product copy is English-first
- Library watcher is intentionally removed
- New libraries auto-scan once after creation when enabled
- New, renamed, moved, or deleted files are reconciled by manual `Scan Library`

## Database Change Rule

- Editing `migrations/0001_init.sql` does not update existing databases that already applied the migration
- For schema changes, be explicit about which path is being used:
  - add a new migration for existing-database compatibility
  - or require a database rebuild / reset when the project is in destructive pre-1.0 cleanup mode
- If the change requires rebuilding `data/postgres` or reinitializing the database, say so clearly in the final explanation

## Backend Rules

- `apps/mova-server` is HTTP/bootstrap/runtime glue only
- Put business logic in `crates/mova-application`
- Put SQL and persistence in `crates/mova-db`
- Put shared domain models in `crates/mova-domain`
- Put scan/parse/probe/sidecar logic in `crates/mova-scan`
- Keep routes under `/api`

## Frontend Rules

- `apps/mova-web` is a standalone Vite app
- Keep features as `feature-name/index.tsx` plus `feature-name.scss`
- Use arrow functions across frontend code, including `src/lib`
- Reuse shared UI primitives instead of re-implementing interactions
- If a control looks raw or awkward, polish it in the same pass

## Frontend Test Strategy

- Avoid low-value page/component `tsx` tests
- Prefer pure helper and hook tests
- Move decision logic into `src/lib/` when that makes testing simpler
- Keep `tsx` tests for high-risk stateful flows like realtime and player behavior

## Validation

- Frontend: run the relevant `biome check`, `tsc -b --pretty false`, and `vite build`
- Backend: prefer Docker-first targeted `cargo check -p ...` and focused tests
- If an API change affects the frontend, validate both sides
- Do not claim runtime behavior unless it was actually exercised

## Scope Of This Skill

- Use this file for codebase map, execution flow, validation, and repo-specific implementation habits.
- Do not use this file as a second roadmap or a second product spec.
- Keep stable collaboration policy in `AGENTS.md` and evolving product direction in `docs/ROADMAP.md`.

## Markdown Sync

- Markdown sync is part of the same task
- Feature or behavior changes: update `README.md` and `docs/ROADMAP.md`
- API changes: update `docs/API.md`
- Frontend structure/responsibility changes: update `apps/mova-web/README.md`
- Backend startup/routes/runtime changes: update `apps/mova-server/README.md`
- Crate responsibility changes: update the affected `crates/*/README.md`

## Commit Style

- Use conventional commits:
  - `feat(scope): ...`
  - `fix(scope): ...`
  - `refactor(scope): ...`
  - `docs(scope): ...`
  - `chore(scope): ...`
- Keep scope concrete, such as `player`, `scan`, `settings`, `libraries`, `auth`, or `api`
