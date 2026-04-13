---
name: mova-workspace
description: Work inside the Mova self-hosted media server repository. Use when changing Rust backend code, the React/Vite frontend, Docker Compose setup, Postgres-backed media-library features, playback UI, scanning logic, or related docs.
---

# Mova Workspace

Use this skill for work in the current Mova repository.

## Minimal Read Order

- Start with `README.md`
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
- Libraries do **not** auto-scan anymore
- New, renamed, moved, or deleted files are reconciled by manual `Scan Library`

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
- Backend: run targeted `cargo check -p ...` and focused tests
- If an API change affects the frontend, validate both sides
- Do not claim runtime behavior unless it was actually exercised

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
