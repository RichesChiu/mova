# Scripts AGENTS

- Treat script inputs, structured outputs, exit codes, and timeouts consumed by Rust or CI as stable
  machine contracts. Keep stdout machine-readable and send optional diagnostics to stderr.
- Keep bounded external-process execution, media-analysis deadlines, and deterministic intro
  detection behavior covered by `mova-scan` tests.
- Preserve multi-platform manifests, image inspection, build-version, alias promotion, and proxy
  behavior in `publish-docker-images.sh`. Application releases reuse base images that already
  contain every required platform and pin every build/runtime base by its multi-platform manifest
  digest.
  Set `MOVA_PUBLISH_BASE_IMAGES=1` only when every base image must be deliberately rebuilt with
  `--pull --no-cache`; scheduled runtime security refreshes own that maintenance independently of
  application releases.
- Preserve per-platform provenance/SBOM attestations in both automated and recovery builds. Validate
  that every real platform manifest has an attached attestation before promotion or reuse.
- Keep `MOVA_VERIFY_IMAGE_REF` pinned to an immutable digest and non-mutating; release retries use
  it to rerun the same runtime smoke and security gates without overwriting a SemVer image tag.
- Keep `MOVA_SMOKE_TEST_SCRIPT` limited to executable, non-symlinked scripts inside `scripts/`;
  application and runtime-base publication share the security gate but use separate smoke suites.
- Candidate cleanup may delete only exact Docker Hub `publish-*` tags by name. Never delete a
  manifest by digest, an immutable SemVer tag, or a mutable channel alias such as `latest` or
  `preview`. Preserve the bounded retention period for failed candidates.
- Preserve the release security gate: report all critical/high findings, block fixable findings and
  CISA KEV entries, and require an exact reviewed CVE set or evidence-backed VEX for unpatched
  residual findings. Do not add broad vulnerability bypasses.
- Add system dependencies only when necessary and document their runtime impact.
