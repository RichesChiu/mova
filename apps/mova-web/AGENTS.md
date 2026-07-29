# Mova Web AGENTS

These instructions apply to the React client under `apps/mova-web`.

- Move reusable decisions into `src/lib/`; keep pages focused on query orchestration and rendering.
- Reuse shared components, design tokens, localization helpers, and interaction patterns before
  creating feature-local variants.
- Route all user-facing copy through the existing i18n catalog.
- Reuse the shared glass surfaces for dialogs, popovers, and menus. When an overlay is clipped,
  inspect overflow, stacking contexts, and portal ownership before changing `z-index`.
- Add focused tests for pure decisions and high-risk interactions such as realtime synchronization,
  playback, and complex overlays. Avoid low-value page snapshots.
- Run the relevant Vitest tests plus:

```bash
pnpm -C apps/mova-web exec tsc -b --pretty false
pnpm -C apps/mova-web build
```
