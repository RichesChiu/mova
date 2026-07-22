# Mova Web AGENTS

These instructions apply to the React and Vite client under `apps/mova-web`. Repository-wide rules live in the root `AGENTS.md`; this file defines frontend implementation and verification details.

## Code organization

- Use `feature-name/index.tsx` with `feature-name.scss` by default.
- Move complex, testable decisions into `src/lib/` rather than embedding them in page rendering.
- Use arrow functions consistently, including in `src/lib`.
- Reuse shared components, design tokens, and interaction patterns before creating local variants.

## Visual and interaction quality

- Fix raw or browser-default-looking controls as part of the same change that exposes them.
- Preserve stable layout and readable hierarchy instead of forcing excessive information into a card or toolbar.
- Reuse shared styles or components for tags, buttons, switches, icon buttons, dialogs, popovers, and menus.
- Use the established thick glass surface for overlays; avoid surfaces that are too transparent to read.
- Prefer `flex` or `inline-flex` for alignment and control groups. Do not simulate layout with arbitrary padding, margins, line-height, or absolute positioning.
- Keep cards visually stable, with primary information, state, and primary actions readable at supported widths.
- Move disruptive preview content into a secondary panel rather than allowing it to break the primary layout.

## Overlays

- Reuse the shared glass surface for modals, popovers, and menus.
- Promote a local overlay into a shared component when another reuse is likely.
- When an overlay is clipped or obscured, inspect stacking contexts, overflow, and section layering before increasing `z-index`.

## Tests and verification

- Prefer tests for pure functions, hooks, and state decisions over low-value page snapshots.
- Use TSX interaction tests for high-risk flows such as realtime updates, playback, and complex overlays.
- After frontend changes, run at least:
  - `pnpm -C apps/mova-web exec tsc -b --pretty false`
  - `pnpm -C apps/mova-web build`
- Run the relevant Vitest suite for changed behavior, and run the full suite before a preview or stable release.
