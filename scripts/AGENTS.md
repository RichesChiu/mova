# Scripts AGENTS

- Treat script inputs, structured outputs, exit codes, and timeouts consumed by Rust or CI as stable
  machine contracts. Keep stdout machine-readable and send optional diagnostics to stderr.
- Changes to `detect_intro.py` must update its Rust caller and contract tests together.
- Preserve multi-platform manifests, image inspection, build-version, alias promotion, and proxy
  behavior in `publish-docker-images.sh`. Set `MOVA_PUBLISH_BASE_IMAGES=1` only when base images must
  be deliberately republished.
- Add Python packages or system dependencies only when necessary and document their runtime impact.
