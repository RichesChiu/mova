# Scripts AGENTS

- Treat script inputs, structured outputs, exit codes, and timeouts consumed by Rust or CI as stable
  machine contracts. Keep stdout machine-readable and send optional diagnostics to stderr.
- Changes to `detect_intro.py` must update its Rust caller and contract tests together.
- Preserve multi-platform manifests, image inspection, build-version, alias promotion, and proxy
  behavior in `publish-docker-images.sh`. Formal releases must refresh the runtime base without
  cached package layers. Set `MOVA_PUBLISH_BASE_IMAGES=1` only when every base image must be
  deliberately republished.
- Preserve the release security gate: report all critical/high findings, block fixable findings and
  CISA KEV entries, and require an exact reviewed CVE set or evidence-backed VEX for unpatched
  residual findings. Do not add broad vulnerability bypasses.
- Add Python packages or system dependencies only when necessary and document their runtime impact.
