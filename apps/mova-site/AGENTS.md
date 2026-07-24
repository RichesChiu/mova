# Mova Site AGENTS

These instructions apply to the official React and Vite website under `apps/mova-site`. Repository-wide workflow and release rules remain owned by the root `AGENTS.md` and `CONTRIBUTING.md`.

## Ownership

- Own `mova.hk`, public product positioning, deployment guidance, API discovery, legal pages, support content, and App Store submission copy stored with the site.
- Keep public claims aligned with capabilities that exist in the repository. Do not advertise planned providers, clients, playback modes, or deployment behavior as already available.
- Keep endpoint listings in `src/data/apiDocs.ts` and their related bilingual public copy synchronized with every `docs/API.md` change; the API document remains the source of truth.
- Keep the custom-domain file at `public/CNAME`. Do not add workflows under this directory because GitHub only loads Actions from the repository-root `.github/workflows/`.

## Implementation

- Reuse the existing site components, icons, responsive patterns, and bilingual content structure before introducing alternatives.
- Keep the website package independent from `apps/mova-web`; it uses npm and its own lockfile.
- Put website-specific development and content guidance in `apps/mova-site/README.md`, not in the root product README.

## Verification

Run the relevant checks from the repository root:

```bash
npm --prefix apps/mova-site ci
npm --prefix apps/mova-site run check:api-docs
npm --prefix apps/mova-site run lint
npm --prefix apps/mova-site run typecheck
npm --prefix apps/mova-site run build
```

For content-only changes, run at least the API documentation check when endpoint copy changes and a production build for any page, asset, route, or configuration change.

After API documentation or website changes reach `master`, verify that the root `Deploy Site` GitHub Action completed successfully. If its automatic path-based trigger did not run, dispatch the workflow manually and verify the deployment before reporting the website update as complete.
