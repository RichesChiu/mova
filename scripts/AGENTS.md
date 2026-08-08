# Scripts AGENTS

- Treat script inputs, structured outputs, exit codes, and timeouts consumed by Rust or CI as stable
  machine contracts. Keep stdout machine-readable and send optional diagnostics to stderr.
- Keep bounded external-process execution, media-analysis deadlines, and deterministic intro
  detection behavior covered by `mova-scan` tests.
- Preserve multi-platform manifests, image inspection, build-version, alias promotion, and proxy
  behavior in `publish-docker-images.sh`. Formal releases must refresh the runtime base without
  cached package layers. Set `MOVA_PUBLISH_BASE_IMAGES=1` only when every base image must be
  deliberately republished.
- Keep `MOVA_VERIFY_IMAGE_REF` pinned to an immutable digest and non-mutating; release retries use
  it to rerun the same runtime smoke and security gates without overwriting a SemVer image tag.
- Preserve the release security gate: report all critical/high findings, block fixable findings and
  CISA KEV entries, and require an exact reviewed CVE set or evidence-backed VEX for unpatched
  residual findings. Do not add broad vulnerability bypasses.
- Add system dependencies only when necessary and document their runtime impact.
