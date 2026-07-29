# Mova Site AGENTS

These instructions apply to the official website under `apps/mova-site`.

- Keep public claims aligned with capabilities present in the repository; planned clients,
  providers, and deployment modes must not be presented as available.
- Keep Simplified Chinese and English public copy synchronized.
- Treat `docs/API.md` as the API source of truth and update `src/data/apiDocs.ts` plus its
  localized public copy in the same change.
- Preserve `public/CNAME`. GitHub Actions belong only in the repository-root `.github/workflows/`.
- Keep this npm package independent from `apps/mova-web` and reuse its existing site components,
  icons, and responsive patterns.
- Follow `apps/mova-site/README.md` for verification. After website changes reach `master`, verify
  the root `Deploy Site` workflow and dispatch it manually when the path trigger did not run.
