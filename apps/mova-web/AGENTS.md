# Mova Web AGENTS

These instructions apply to the React client under `apps/mova-web`.

- Keep pages focused on query orchestration and rendering. Move reusable business decisions into
  `src/lib/`, and reusable UI into shared components and design tokens.
- Route all user-facing copy through the i18n catalog and keep Simplified Chinese and English
  entries synchronized.
- Reuse shared dialog, menu, popover, select, and tooltip primitives. For clipped overlays, inspect
  portal ownership, overflow, and stacking contexts before changing `z-index`.
- Test observable behavior, pure decisions, and high-risk flows such as realtime synchronization,
  playback, permissions, and keyboard interaction. Do not retain tests that only assert static copy,
  CSS classes, DOM shape, simple wrappers, or purely visual styling.
