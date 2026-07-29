# Scripts AGENTS

- Treat script inputs, structured outputs, exit codes, and timeouts consumed by Rust as stable machine contracts.
- Keep stdout machine-readable; send optional human diagnostics to stderr.
- Changes to `detect_intro.py` must update its Rust caller and contract tests together.
- Preserve multi-platform, base-image inspection, build-version, and proxy behavior in `publish-docker-images.sh`.
- Add Python packages or system dependencies only when necessary and document the runtime impact.
- Run the narrowest script check and the affected Rust checks.
