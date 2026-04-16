# AGENTS

This file is intentionally short. It exists to capture the highest-priority AI collaboration rules for this repository without duplicating `README.md`, `docs/ROADMAP.md`, or `.codex/skills/mova-workspace/SKILL.md`.

If guidance conflicts, follow this priority:
1. direct user instructions
2. `AGENTS.md`
3. repo skill guidance
4. supporting project docs

## Working Rules

- Prefer Docker-first workflows. Do not require the host machine to install Rust or Python unless there is no practical alternative.
- Keep user-facing product copy English-first unless the task explicitly calls for another language.
- When implementing features during the current development phase, update the relevant markdown docs in the same change when behavior, APIs, setup, or product direction changes.
- Do not preserve compatibility for removed fields or removed UI during this phase. Remove obsolete code directly instead of layering compatibility shims.
- Prefer more aggressive cleanup/refactor behavior during implementation. If a path is obsolete and clearly replaced, delete it rather than keeping fallback logic.
- Prefer deletion over migration-heavy compatibility work when the project is clearly replacing an obsolete path during this pre-1.0 phase.
- Before committing, run the relevant scoped verification for the area you changed, such as `cargo check`, frontend `tsc`, frontend build, or targeted tests.
- When uncertain, inspect the code first. Do not rely on assumptions or stale context when changing behavior.
- For database schema changes, explicitly call out whether existing databases can migrate safely or whether a database rebuild / data directory reset is required.

## Project Structure

- `apps/mova-server`
  Rust HTTP server and runtime entrypoint.
- `apps/mova-web`
  React + Vite frontend.
- `crates/mova-application`
  Application-layer business logic.
- `crates/mova-db`
  SQL queries, persistence, and sync logic.
- `crates/mova-domain`, `crates/mova-scan`
  Shared models and media discovery/probing.
- `migrations`
  Database schema migrations.
- `scripts`
  Helper scripts, including Python-based media analysis tasks.

## Notes For AI Contributors

- Read code paths near the change before editing.
- Keep modifications aligned with the current product direction rather than preserving outdated behavior.
- If behavior changed, document the new expected behavior clearly.
