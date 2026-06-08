# Mova Docker Base Images

These Dockerfiles define the build and runtime base images used by `apps/mova-server/Dockerfile`.

The normal release entrypoint is the repository-level publish script. It targets `linux/amd64` and `linux/arm64` by default, checks whether these base image tags already contain the required platforms, publishes missing base images, and then publishes `richeschiu/mova:latest`:

```sh
./scripts/publish-docker-images.sh
```

Force rebuilding and publishing all base images before the main image:

```sh
MOVA_PUBLISH_BASE_IMAGES=1 ./scripts/publish-docker-images.sh
```

Build and publish only the base images manually from the repository root when needed:

```sh
docker buildx build --platform linux/amd64,linux/arm64 -f docker/base/web-build.Dockerfile -t richeschiu/mova-web-build-base:node24-pnpm11 --push .
docker buildx build --platform linux/amd64,linux/arm64 -f docker/base/rust-build.Dockerfile -t richeschiu/mova-rust-build-base:1-bookworm --push .
docker buildx build --platform linux/amd64,linux/arm64 -f docker/base/runtime.Dockerfile -t richeschiu/mova-runtime-base:bookworm-ffmpeg-python3 --push .
```

The main release image can also be published manually:

```sh
docker buildx build --platform linux/amd64,linux/arm64 -f apps/mova-server/Dockerfile -t richeschiu/mova:latest --push .
```
