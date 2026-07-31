# 第三方软件与对应源码

Mova 的应用源码使用仓库根目录 `LICENSE` 中的 AGPL-3.0-only 许可证。官方服务端容器还包含 Debian 发行的软件包；这些独立组件继续适用各自的许可证。

## Debian 与 FFmpeg

官方镜像的运行时基于 Debian 13（trixie），并通过 Debian `main` 安装 FFmpeg、Python 和 CA 证书。Debian 的 FFmpeg 构建启用了 GPL 功能，因此容器内的 FFmpeg 可执行文件按照 Debian 软件包随附的 GPL 条款分发。

每个 Mova 运行时镜像都保留：

- `/usr/share/doc/<package>/copyright`：Debian 软件包的版权和许可证信息；
- `/usr/share/mova/third-party/debian-packages.tsv`：该镜像实际安装的软件包和精确版本；
- `/usr/share/mova/third-party/ffmpeg-source.txt`：FFmpeg 二进制版本及对应源码入口。

Debian 为官方发行的软件包提供完整对应源码和 Debian 修改：

- [Debian FFmpeg source package](https://packages.debian.org/source/trixie/ffmpeg)
- [Debian Sources: FFmpeg](https://sources.debian.org/src/ffmpeg/)
- [Debian source package instructions](https://www.debian.org/doc/manuals/debian-handbook/sect.source-package-structure.en.html)

需要复现特定 Mova 镜像时，先读取镜像内的精确版本清单，再从 Debian source package 页面下载同版本的 `.dsc`、原始源码压缩包和 `debian.tar.*`。Mova 未修改 Debian 提供的 FFmpeg 源码。

## TMDB

TMDB 的商标、Logo、元数据和图片不属于 Mova。其使用方式、归属声明和本地保留策略见 [`TMDB_INTEGRATION.md`](TMDB_INTEGRATION.md)。
