# Mova

Mova 是一个自托管媒体服务器项目，目标是把本地媒体目录整理成可扫描、可浏览、可播放、可持续同步的媒体库。

当前仓库包含后端服务 `mova-server` 和前端原型 `mova-web`。后端负责媒体库管理、文件扫描与增量同步、元数据补全、剧集聚合、图片缓存、播放进度和基础流媒体能力；前端负责把这些能力组织成可直接验证的管理与浏览界面。

## 主要技术

- Rust workspace
- React
- Vite
- Axum
- Tokio
- PostgreSQL
- SQLx
- TanStack Query
- `notify` 文件系统 watcher
- `ffmpeg` / `ffprobe`
- TMDB 元数据补全
- Biome
- Docker Compose

## 当前能力

- 支持创建 `mixed` / `movie` / `series` 三种媒体库，默认推荐 `mixed`
- 支持重叠库和相同路径重复建库；同一物理文件会在各自库内独立建模
- 创建启用库后会自动触发首轮扫描
- 支持后台扫描任务、扫描历史和单任务状态查询
- 已启用库会通过 watcher 和定时路径校准自动处理新增、删除、改名、移动等常见文件变更
- 扫描按 `(library_id, file_path)` 做增量同步，不再整库替换
- 电影按单文件建模；剧集会聚合成 `series / seasons / episodes`
- 支持本地 sidecar 元数据读取，例如 `.nfo`、`poster.jpg`、`fanart.jpg`
- 支持 TMDB 补全缺失的标题、简介、年份、海报和背景图，并把图片缓存到本地
- 支持媒体列表分页、名称搜索、发行年筛选
- 支持媒体详情、文件列表、海报、背景图、季列表、集列表
- 支持剧集季/集级封面与背景图（TMDB 优先，缺失时集级可回退首帧）
- 支持媒体文件直链播放和基础单段 `Range` 请求
- 支持首个管理员 bootstrap、登录、登出和当前用户查询
- 支持 `admin / viewer` 两类用户；管理员默认可见全部媒体库，普通用户按库授权
- 支持播放进度写入和继续观看列表，且已切到真实登录用户维度
- 设置页已接入最小用户管理，可创建用户并为普通用户分配媒体库访问范围
- 已提供基于 React + Vite 的前端原型，可直接联调媒体库、扫描、详情和剧集聚合

## 启动

当前只保留一种推荐启动方式：

```bash
docker compose up -d --build
```

默认行为：

- 服务地址：`http://127.0.0.1:36080`
- 健康检查：`GET /api/health`
- 示例媒体目录：宿主机 [`dev-media/`](dev-media/) 挂载到容器内 `/media/dev-media`
- 运行时数据目录：[`data/postgres/`](data/postgres/)、[`data/cache/`](data/cache/)

可选：通过 `.env` 同时配置 Docker 挂载源路径和应用内可选媒体根路径：

```env
MOVA_MEDIA_BIND_SOURCE=\\fn-vm\media\mainlan_tv
MOVA_MEDIA_BIND_TARGET=/media/mainlan_tv
MOVA_LIBRARY_ROOTS=/media/mainlan_tv
```

说明：
- `MOVA_MEDIA_BIND_SOURCE` 是宿主机路径，供 Docker bind mount 使用；这里可以是 Windows 路径或 UNC 路径。
- `MOVA_MEDIA_BIND_TARGET` 是容器内路径，`mova-server` 真正看到和使用的是这个路径。
- `MOVA_LIBRARY_ROOTS` 只接受容器内路径，支持用英文逗号、分号或换行分隔多个路径，例如 `/media/mainlan_tv;/media/anime`。
- 不要把 `\\fn-vm\media\mainlan_tv` 直接写进 `MOVA_LIBRARY_ROOTS`；这类宿主机路径只应该出现在 `MOVA_MEDIA_BIND_SOURCE`。
- 如果 Docker 引擎本身无法访问某个宿主机目录或网络共享，应用层配置正确也不会生效。
- 未来如果要增加更多媒体目录，建议在 `docker-compose.yml` 或 `docker-compose.override.yml` 里新增挂载，再把对应容器路径追加到 `MOVA_LIBRARY_ROOTS`。
- 旧的单路径变量 `MOVA_MEDIA_ROOT` 仍兼容，但只建议保留给历史配置。

说明：
- 当前开发阶段只维护 `migrations/0001_init.sql`。如果你在此之前已经启动过旧数据库，升级后建议重建 `data/postgres` 目录再启动，确保新表结构生效。
- 这次用户体系改动直接修改了 `0001_init.sql`。如果你已经有旧库数据，需要先重建 `data/postgres` 再启动。

如果要修改内置的 TMDB token，编辑 [`apps/mova-server/src/embedded_metadata.rs`](apps/mova-server/src/embedded_metadata.rs)，然后重新构建：

```bash
docker compose up -d --build
```

## 文档

- 接口说明：[docs/API.md](docs/API.md)
- 功能现状与开发路线：[docs/ROADMAP.md](docs/ROADMAP.md)
- 工程结构与重构建议：[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)
- 前端原型说明：[apps/mova-web/README.md](apps/mova-web/README.md)
