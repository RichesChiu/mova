# 第三方软件与对应源码

Mova 的应用源码使用仓库根目录 `LICENSE` 中的 AGPL-3.0-only 许可证。官方服务端容器还包含 Debian 发行的软件包；这些独立组件继续适用各自的许可证。

## Debian 与 FFmpeg

官方镜像的运行时基于 Debian 13（trixie）。FFmpeg 和 FFprobe 从固定的 FFmpeg 官方源码提交构建，源码压缩包使用 SHA-256 校验；构建关闭自动发现的外部库、网络协议和 GPL 组件，仅保留 MOVA 本地媒体探测、重封装、字幕转换与片头音频分析所需的命令行能力。片头分析由 Rust 实现，运行时不包含 Python。

每个 Mova 运行时镜像都保留：

- `/usr/share/doc/<package>/copyright`：Debian 软件包的版权和许可证信息；
- `/usr/share/mova/third-party/debian-packages.tsv`：该镜像实际安装的软件包和精确版本；
- `/usr/share/mova/third-party/ffmpeg-source.txt`：FFmpeg 提交、源码地址、校验和与构建配置；
- `/usr/share/mova/third-party/COPYING.LGPLv2.1`、`COPYING.LGPLv3` 与 `LICENSE.md`：FFmpeg 随附的许可证文件。

FFmpeg 的上游仓库、源码快照和许可证说明：

- [FFmpeg 官方仓库](https://github.com/FFmpeg/FFmpeg)
- [FFmpeg 法律与许可证说明](https://ffmpeg.org/legal.html)
- [FFmpeg LGPL 2.1](https://github.com/FFmpeg/FFmpeg/blob/master/COPYING.LGPLv2.1)

需要复现特定 Mova 镜像时，读取镜像内的 `ffmpeg-source.txt`，按照其中的提交与 SHA-256 获取源码，并使用记录的配置参数构建。仓库中的 `docker/base/runtime.Dockerfile` 是可执行的构建定义。

## TMDB

TMDB 的商标、Logo、元数据和图片不属于 Mova。其使用方式、归属声明和本地保留策略见 [`TMDB_INTEGRATION.md`](TMDB_INTEGRATION.md)。
