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

- 本机访问：`http://127.0.0.1:36080`
- 远程服务器访问：`http://<服务器IP>:36080`
- 健康检查：`GET http://<服务器IP>:36080/api/health`
- 示例媒体目录：宿主机 [`dev-media/`](dev-media/) 挂载到容器内 `/media`
- 运行时数据目录：[`data/postgres/`](data/postgres/)、[`data/cache/`](data/cache/)

启动后会同时拉起：

- `mova-server`：Rust API 服务，同时直接托管构建后的前端静态文件
- `database`：PostgreSQL

如果你的服务器地址是 `192.168.50.3`，启动完成后直接访问：

```text
http://192.168.50.3:36080
```

可选：通过 `.env` 配置宿主机媒体根目录：

```env
MOVA_MEDIA_ROOT=/mnt/media
```

说明：
- `MOVA_MEDIA_ROOT` 是宿主机路径，Docker 会把它只读挂载到容器内固定目录 `/media`。
- Linux 部署时，推荐先把 SMB / NFS 等网络共享挂到宿主机本地目录，例如 `/mnt/media`，再把 `MOVA_MEDIA_ROOT` 指向这个目录。
- 前端创建媒体库时会直接展示 `/media` 下的递归目录树；你点击哪个文件夹，就把哪个文件夹作为库源路径。
- 这样用户不需要手写 `/media/...` 路径，也不需要额外维护多套环境变量。
- 当前约定只保留一个宿主机媒体根目录；后续扩展优先通过这个根目录下的子目录来做，而不是再引入更多环境变量。

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
