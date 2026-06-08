<p align="center">
  <img src="apps/mova-web/public/mova-logo-master-transparent.png" alt="Mova 标志" width="96" />
</p>

<h1 align="center">Mova</h1>

<p align="center">
  面向本地电影和剧集的轻量、安全、高效自托管媒体服务器。
</p>

<p align="center">
  <a href="README.md">英文版</a> | 中文
</p>

![Mova 主页](docs/assets/readme/home.png)

## Mova 是什么

Mova 是一个用于整理、浏览和播放本地电影与剧集的自托管媒体服务器。服务端使用 Rust 构建，这是一门强调内存安全、稳定性能和资源效率的现代系统语言。

项目希望把媒体服务器体验保持得足够简单可靠：挂载媒体目录，扫描媒体库，按需补齐元数据，然后在清晰的网页界面里浏览和播放。当前版本定位为可用的 MVP，适合本机、家用服务器和私人媒体库场景。

Web 主页以媒体库为第一层级：展示继续观看、简短的 `你的库` 摘要，以及由服务端按库聚合的最新添加内容，而不是前端把各库按标题排序后的列表再混合拼出来。

对于本地媒体很少的机器，Web 端提供一个明确的开发期 mock API 开关，方便 UI 审核。开关说明见 [apps/mova-web/README.md](apps/mova-web/README.md)，默认关闭，因此真实 API 错误不会被 mock 数据掩盖。

剧集归组会优先且只信任文件名。建议使用 `剧名.S01E01.mkv`、`剧名 S01E01 - 第 1 集.mkv`、`剧名 - S01E01.mkv`、`剧名_S01E01.mkv`、`剧名S01E01.mkv` 这类命名；Mova 不会从随意命名的文件夹里推断剧集身份。如果文件位于明确季目录下，例如 `流氓读书会 (2025)/第 1 季/Study Group S01E01.mkv`，父级剧集目录里的年份只会作为元数据搜索提示使用。TMDB 补全成功前，卡片先使用本地分析出的电影或剧集名称；TMDB 补全成功后，再用 TMDB 返回的名称覆盖本地名称。电影文件只要最终绑定到同一个 TMDB 影片，就会归并到同一个详情页作为多个本地版本，即使本地目录名或标点不同；如果电影文件名和干净的中文父目录不一致，中文父目录只会作为后备 TMDB 查询候选。没有季集身份的文件会同时参考 TMDB 电影和剧集搜索结果；如果远端更像剧集但本地没有季集号、远端匹配失败或文件名明显异常，会用明确的元数据复核状态入库并进入 Other 分区。如果未启用 TMDB，元数据状态会标记为 skipped，本地识别出的电影或剧集仍会正常展示。

一次成功扫描后，后续扫描会先按文件路径匹配，再比较由文件大小和修改时间生成的轻量指纹。扫描拆成四段：发现物理文件、浅层文件名聚合、按组本地分析、TMDB 元数据补全。浅层阶段只读取文件名和路径，用来先建立稳定的电影/剧集组，不读取 sidecar，也不调用 `ffprobe`；随后每个组再做完整本地分析、写库并推送给前端，然后才继续处理下一组。本地分析会保存自己的版本号，所以只有文件指纹和本地分析版本都一致时，才会跳过拆名、sidecar 读取、`ffprobe` 探测和聚合。如果条目从未成功绑定 TMDB、位于 Other、之前匹配失败、曾因 TMDB 未启用而跳过，或只保存了还没缓存成本地文件的远端图片 URL，Mova 会复用已入库的本地分析结果，直接进入逐条 TMDB 补全。自动匹配保持保守，更宽泛的候选复核交给手动元数据搜索流程。图片字段各自保持自己的语义：剧集、季、单集、海报和背景图不会互相替代，也不会跨层级兜底。已经匹配且未变化的条目会保持稳定，即使 TMDB 当前没有海报也不会拿其它图片补齐。本地占位条目会按组写入，但待完成的本地写入不会清空已有图片；只有最终 `matched` 元数据写入确认远端确实缺图时，才会清空对应图片字段。每成功补齐一个 TMDB 条目就立即覆盖写库，因此海报会逐个出现。

当运行环境可用 `ffprobe` 时，Mova 也会为每个物理资源文件保存 4K、1080p、HDR10、Dolby Vision、DTS-HD、Atmos 等资源级技术标签，并在详情页以资源徽标展示。

## 截图

### 详情页和浅色主题

![Mova 详情页浅色主题](docs/assets/readme/theme.png)

### 服务器设置

![Mova 服务器设置](docs/assets/readme/server-setting.png)

## 部署

### 环境要求

- Docker
- Docker Compose
- 一个宿主机上的本地媒体目录

### 配置

```bash
cp .env.example .env
```

常用配置：

```env
MOVA_MEDIA_ROOT=/absolute/path/to/media
MOVA_TMDB_ACCESS_TOKEN=
MOVA_OMDB_API_KEY=
HTTP_PROXY=
HTTPS_PROXY=
```

- `MOVA_MEDIA_ROOT` 必填，会只读挂载到容器内固定目录 `/media`。
- `MOVA_TMDB_ACCESS_TOKEN` 可选，不填也能扫描、入库和播放。
- `MOVA_OMDB_API_KEY` 可选，配置后会在拿到 `imdb_id` 时补 IMDb 评分。

### 启动

```bash
docker compose up -d
```

默认地址：

- Web: `http://127.0.0.1:36080`
- Health: `http://127.0.0.1:36080/api/health`

启动后，Mova 会生成两个运行时目录：

- `data/postgres/`：PostgreSQL 数据库文件，用于保存媒体库、用户、元数据和播放进度。
- `data/cache/`：缓存海报、背景图和生成的媒体资源。删除媒体库时，也会清理该库独占引用的 TMDB 图片缓存。

当前仍处于 pre-MVP 开发阶段，数据库 schema 变化时可能需要重建 `data/postgres/`。当前 `migrations/0001_init.sql` 会保存本地分析版本，并把 TMDB / provider 返回的文本字段作为不固定长度的文本存储，已有开发数据库拉取后应重建。

媒体目录只读挂载，Mova 不会修改你的原始媒体文件。

默认 Compose 文件会直接运行已发布的 `richeschiu/mova:latest` 镜像，不在部署机器上从源码构建。本地没有镜像时，`docker compose up -d` 会自动拉取；如果你想主动升级到最新发布镜像，自己先执行 `docker compose pull`，再执行 `docker compose up -d`。

如果需要本地源码构建，在本机 `.env` 里设置：

```dotenv
COMPOSE_FILE=docker-compose.yml:docker-compose.build.yml
```

之后本地启动也可以使用同样简短的形式：

```bash
docker compose up -d --build
```

已发布镜像和构建基础镜像默认是 Linux 多架构镜像，覆盖 `linux/amd64` 和 `linux/arm64`。Windows 和 macOS 宿主机通过 Docker Desktop 运行同一个 Linux 镜像，Linux 宿主机通过 Docker Engine 或 Docker Desktop 运行同一镜像，Docker 会自动选择匹配的架构。发布入口是 `./scripts/publish-docker-images.sh`；脚本会检查构建基础镜像 tag 是否已经包含所需平台，缺失时先发布基础镜像，再推送 `richeschiu/mova:latest`。

应用服务名是 `app`，运行时容器固定为 `mova-app`；查看服务日志时使用 `docker compose logs -f app`。

### 首次使用

1. 容器启动后打开 Web 页面。
2. 在初始化页面创建第一个管理员。
3. 进入服务器设置并创建媒体库。
4. 选择容器内 `/media` 下的目录。
5. 保存媒体库后，Mova 会自动开始第一次扫描。

## 文档

- API: [docs/API.md](docs/API.md)
- 前端: [apps/mova-web/README.md](apps/mova-web/README.md)
- 后端: [apps/mova-server/README.md](apps/mova-server/README.md)
- Crates: [crates/README.md](crates/README.md)

## 路线图与反馈

Mova 仍在积极迭代中。作者也在积极维护 Pad 和 macOS 客户端方向，让它们可以更自然地接入同一个自托管媒体服务器。

欢迎提交反馈、功能建议、客户端接入想法和体验改进意见。

## 许可证

当前许可证：`AGPL-3.0-only`。详见 [LICENSE](LICENSE)。
