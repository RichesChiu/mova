# Mova Docker Base Images

These Dockerfiles provide the reusable layers consumed by
`apps/mova-server/Dockerfile`:

- `web-build.Dockerfile`: Node.js and pnpm Web build environment
- `rust-build.Dockerfile`: Rust build environment
- `runtime.Dockerfile`: runtime system, FFmpeg, and Python

Publish application images through the repository script. It verifies the
required `linux/amd64` and `linux/arm64` base-image platforms and publishes
missing base images automatically:

```sh
MOVA_DOCKER_IMAGE_TAG=richeschiu/mova:<immutable-tag> ./scripts/publish-docker-images.sh
```

Force rebuilding all base images only after intentionally changing their
toolchain or runtime contents:

```sh
MOVA_PUBLISH_BASE_IMAGES=1 \
MOVA_DOCKER_IMAGE_TAG=richeschiu/mova:<immutable-tag> \
./scripts/publish-docker-images.sh
```
