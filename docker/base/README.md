# Mova Docker Base Images

These Dockerfiles provide the reusable layers consumed by
`apps/mova-server/Dockerfile`:

- `web-build.Dockerfile`: Node.js and pnpm Web build environment
- `rust-build.Dockerfile`: Rust build environment
- `runtime.Dockerfile`: Debian 13 runtime with pinned, source-built FFmpeg and FFprobe

Publish application images through the repository script. It verifies the
required `linux/amd64` and `linux/arm64` base-image platforms and publishes
missing build images automatically. A formal release always rebuilds the
runtime base with `--pull --no-cache` so Debian security updates cannot be
hidden by a cached `apt` layer:

```sh
MOVA_DOCKER_IMAGE_TAG=richeschiu/mova:<immutable-tag> ./scripts/publish-docker-images.sh
```

Formal publishes require an immutable SemVer image tag, a clean Git worktree
and index, and an annotated `v<version>` Git tag pointing to `HEAD`. For a
deliberate development-only publish that does not meet those release
conditions, opt in explicitly:

```sh
MOVA_ALLOW_UNRELEASED=1 \
MOVA_DOCKER_IMAGE_TAG=richeschiu/mova:development \
./scripts/publish-docker-images.sh
```

Force rebuilding all base images only after intentionally changing their
toolchain contents. The runtime base is already refreshed automatically for a
formal release:

```sh
MOVA_PUBLISH_BASE_IMAGES=1 \
MOVA_DOCKER_IMAGE_TAG=richeschiu/mova:<immutable-tag> \
./scripts/publish-docker-images.sh
```

The runtime image builds FFmpeg and FFprobe from a pinned upstream commit whose
archive is SHA-256 verified. The configuration disables network protocols,
external-library autodetection, and GPL components; Mova uses the tools only for
local media probing, remuxing, subtitle conversion, and intro-audio extraction.
Intro analysis is implemented in Rust, so Python is not present. The final image
installs only the Debian CA certificate package and removes `perl-base` after
package maintenance completes. Runtime images are immutable: rebuild the base
image to apply package updates instead of installing packages in a running
container.

Publishing requires Docker Scout and local support for running every requested
platform. The script first pushes a uniquely tagged multi-platform candidate,
pins its manifest digest, smoke-tests and scans that immutable digest on every
requested platform, and only then promotes the same digest to the release tag.

The security gate always reports unfixed critical and high findings. It blocks
every fixable critical or high finding and every finding in the CISA Known
Exploited Vulnerabilities catalog. A residual finding without an upstream fix
must either be suppressed by a reviewed VEX statement or be named exactly in
`MOVA_ACCEPT_UNFIXED_CVES` after a maintainer risk review. For example:

```sh
MOVA_ACCEPT_UNFIXED_CVES=CVE-YYYY-NNNN,CVE-YYYY-NNNN \
MOVA_DOCKER_IMAGE_TAG=richeschiu/mova:<immutable-tag> \
./scripts/publish-docker-images.sh
```

Use `MOVA_SCOUT_VEX_LOCATION` for a reviewed VEX file or directory and
`MOVA_SCOUT_VEX_AUTHORS` for its comma-separated author patterns. Never use VEX
for a merely unpatched or low-likelihood finding; it is reserved for evidence
that the vulnerable code path is not present or reachable.

A failed build, smoke test, scan, risk-set comparison, or digest resolution
leaves the existing release tag unchanged. The script also verifies that the
promoted release tag resolves to the approved candidate digest.
