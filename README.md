<p align="center">
  <img src="apps/mova-web/public/mova-logo-master-transparent.png" alt="Mova 标志" width="96" />
</p>

<h1 align="center">Mova</h1>

<p align="center">
  面向本地电影和剧集的轻量、安全、高效自托管媒体服务器。
</p>

## Mova 是什么

Mova 是一个用于整理、浏览和播放本地电影与剧集的自托管媒体服务器。服务端使用 Rust 构建，这是一门强调内存安全、稳定性能和资源效率的现代系统语言。

项目希望把媒体服务器体验保持得足够简单可靠：挂载媒体目录，扫描媒体库，按需补齐元数据，然后在清晰的网页界面里浏览和播放。当前版本定位为可用的 pre-1.0 MVP 预览版，适合本机、家用服务器和私人媒体库场景。

Web 主页以媒体库为第一层级：有继续观看数据时才展示该区块；`你的库` 最多在一排展示 5 个库，超过 5 个时才提供查看全部；最新添加由服务端按每个可见非空媒体库返回最多 8 条内容，不设置默认时间窗口。首页和全部媒体库页共用同一个库卡组件，管理员在两处都可以通过右上角三点菜单编辑、扫描或删除媒体库。

Web 界面在首次初始化或没有有效语言偏好时默认使用简体中文；用户在个人设置中明确选择的语言仍会保存在当前浏览器。

媒体库编辑器只允许修改名称、描述和 TMDB 元数据语言，创建后的根路径保持只读。媒体库不再维护启用/禁用状态：新建库始终自动开始首次扫描，所有已有库也始终可以手动扫描。切换元数据语言需要二次确认，确认后会复用未变化文件的本地分析结果，并按新语言自动执行一次全库元数据刷新。

服务器设置中的媒体库卡片将超长标题压缩为单行省略，并用更小字号为描述固定保留两行省略高度；悬浮标题、描述或根路径会立即显示带指向箭头的完整内容气泡，默认出现在目标上方，顶部空间不足时自动翻到下方，并避免超出左右边界。扫描状态紧凑地放在垂直居中的三点菜单左侧；扫描成功只保留绿色状态点和文字，不再给卡片增加绿色底色。编辑、扫描和删除统一收进与首页库卡片一致的三点菜单。

原生客户端使用 opaque 短期 access token 和可轮换 refresh token 登录续期。业务 API 只接受 `Authorization: Bearer ...` 里的 access token；refresh token 以 hash 形式保存在服务端，可按设备会话撤销，并且只通过 `/api/auth/refresh` 使用。

Web、macOS 和 iOS 共用服务端定义的 Realtime/SSE v1 协议。`GET /api/home` 返回有界首页快照，不再让客户端为首页下载每个媒体库的完整目录；PostgreSQL 中持久化的资源 revision 是可靠变更状态，SSE 只发送批量失效提示和临时扫描进度。扫描任务级 `progress_percent` 同样持久化在 PostgreSQL，并由服务端按物理文件单调推进：文件树、增量计划和浅层分组全部完成为 10，本地分析贡献 20，pending 入库贡献 20，远端处理贡献 49，最终成功提交才到 100。local 与 remote 会有界重叠，因此进度不会刻意停在 50，客户端也不按 phase 或条目数量自行估算。扫描期间 Web 合并普通 catalog 失效，只在本地检查点和最终完成时刷新正式目录。最后一跳按 public、admin、library、user scope 分域，单个用户或媒体库的高频事件不会唤醒无关连接。客户端重连或从后台回到前台时通过 `GET /api/realtime/state` 恢复，只刷新 revision 发生变化的资源。媒体库扫描以 PostgreSQL 持久化后台任务入队，由有并发上限的 worker 池领取，HTTP 请求不再持有扫描生命周期，服务重启后未完成任务仍可继续领取。

登录后的顶部铃铛是通用通知中心，不与扫描功能绑定。通知按 `scan`、`system`、`library`、`account` 分类，统一保存事件类型、级别、可见范围、扩展 payload 和每个用户独立的已读状态；未知类别和事件类型可以在不修改基础表结构的情况下继续扩展。扫描只是首个通知生产者：每个远端扫描组成功提交后由 worker 在任务内存中累计匹配、未匹配、失败和 `ffprobe` 警告，任务结束时把统计和最多 20 个问题摘要直接写入一条 `scan` 通知。原始排障信息继续输出到 Rust `tracing` / Docker 日志，不另建扫描报告表或报告接口。SSE 只通知客户端失效 `notifications` 资源，不承载或回放完整通知内容。

登录账户可以使用普通账号名，也可以使用最长 254 个字符的邮箱形式字符串。Mova 只把它作为精确匹配的账户标识，不会验证邮箱归属或发送邮件。

对于本地媒体很少的机器，Web 端提供一个明确的开发期 mock API 开关，方便 UI 审核。开关说明见 [apps/mova-web/README.md](apps/mova-web/README.md)，默认关闭，因此真实 API 错误不会被 mock 数据掩盖。

识别出完整季集坐标后，剧集标题和年份按 `tvshow.nfo`、文件名的顺序确定。建议使用 `剧名.S01E01.mkv`、`剧名 S01E01 - 第 1 集.mkv`、`剧名 - S01E01.mkv`、`剧名_S01E01.mkv`、`剧名S01E01.mkv` 这类命名；目录路径只作为同一剧集的分组边界，目录文字不会成为标题或年份候选。第一季文件中的明确年份表示系列首播年；第二季及以后文件中的年份只表示对应季播出年，不能覆盖系列首播年。导入包含第一季的剧集时忽略后续季年份；只导入后续季且没有首播年时，TMDB search 使用季播出年缩小结果，再读取对应季详情严格验证。电影文件名开头的数字默认属于标题；只有文件直接位于名称明确的“合集 / 系列 / Collection / Box Set / Saga”目录或 `Season 01 / S01 / 第 1 季` 目录时，才会把 `1.Movie.mkv` 的 `1.` 当作顺序号。TMDB 补全成功前，卡片先使用本地分析出的电影或剧集名称；TMDB 补全成功后，再用 TMDB 返回的名称覆盖本地名称。电影文件只要最终绑定到同一个 TMDB 影片，就会归并到同一个详情页作为多个本地版本，即使本地目录名或标点不同；如果电影文件名和干净的中文父目录不一致，中文父目录只会作为后备 TMDB 查询候选。TMDB endpoint 由文件名中的完整季集坐标唯一决定：有季号和集号只查 TV，否则只查 movie，不跨类型兜底。自动候选必须严格对齐本地化标题、原始标题或 alternative title；别名中的 `$` 只有位于两个 ASCII 英文字母之间时才按风格化 `s` 处理，普通标题不会全局忽略空白。只有本地标题以数字结尾时，才允许 TMDB 在相同主标题后用冒号等明确分隔符追加副标题，不做普通前缀或模糊匹配。电影发行年和剧集首播年必须完全相同；没有作品年份或可验证季年份时，选择正式日期最新的严格候选，最新日期并列时保持未匹配。完整规则见 [TMDB 对接文档](docs/TMDB.md)。如果未启用 TMDB，元数据状态会标记为 skipped，本地识别出的电影或剧集仍会正常展示。

一次成功扫描后，后续扫描会先按文件路径匹配，再比较由文件大小和修改时间生成的轻量指纹。扫描拆成文件发现、浅层文件名聚合、本地分析/pending 入库和远端补全；最后两段由一个 local worker 和一个 remote worker 通过容量为 2 的队列形成有界流水线，组 A 进入 TMDB 和图片处理时，组 B 可以继续 `ffprobe`。浅层阶段只读取文件名和路径，用来先建立稳定的电影/剧集组，不读取 sidecar，也不调用 `ffprobe`。本地分析会保存自己的版本号，所以只有文件指纹和本地分析版本都一致时，才会跳过拆名、sidecar 读取、`ffprobe` 探测和聚合。如果条目从未成功绑定 TMDB、位于 Other、之前匹配失败、曾因 TMDB 未启用而跳过，或只保存了还没缓存成本地文件的远端图片 URL，Mova 会复用已入库的本地分析结果，直接进入 TMDB 补全。TMDB endpoint 只由完整季集坐标决定，自动候选必须严格对齐主标题；数字结尾的续集名可以对齐明确分隔的远端副标题，本地带年份时还必须严格同年，不计算匹配分数，也不跨类型兜底。图片字段各自保持自己的语义：剧集、季、单集、海报和背景图不会互相替代，也不会跨层级兜底。已经匹配且未变化的条目会保持稳定，即使 TMDB 当前没有海报也不会拿其它图片补齐。本地占位条目会按组写入，但 pending 写入不会清空已有图片；只有最终 `matched` 元数据写入确认远端确实缺图时，才会清空对应图片字段。每成功补齐一个 TMDB 条目就立即覆盖写库，因此海报会逐个出现。

同名同年的严格候选有多个时，Mova 优先保留 `original_title / original_name` 也与本地主标题严格对齐的候选。例如本地 `奇遇 (2025)` 会唯一匹配原始标题同样为“奇遇”的中国作品，不会把中文元数据语言误解为制作国家偏好；原始标题优先后的候选仍不唯一时保持未匹配。

当运行环境可用 `ffprobe` 时，Mova 也会为每个物理资源文件保存 4K、1080p、HDR10、Dolby Vision、DTS-HD、Atmos 等资源级技术标签，并在详情页以资源徽标展示。

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
MOVA_WORKER_CONCURRENCY=2
HTTP_PROXY=
HTTPS_PROXY=
```

- `MOVA_MEDIA_ROOT` 必填，会只读挂载到容器内固定目录 `/media`。
- `MOVA_TMDB_ACCESS_TOKEN` 可选，不填也能扫描、入库和播放。
- `MOVA_OMDB_API_KEY` 可选，配置后会在拿到 `imdb_id` 时补 IMDb 评分。
- `MOVA_WORKER_CONCURRENCY` 控制进程内后台 worker 池并发数，默认值为 `2`。

### 启动

```bash
docker compose up -d
```

默认地址：

- Web: `http://127.0.0.1:36080`
- Health: `http://127.0.0.1:36080/api/health`

启动后，Mova 会生成两个运行时目录：

- `data/postgres/`：PostgreSQL 数据库文件，用于保存媒体库、用户、元数据、播放进度、持久化通知与已读状态、后台任务和实时资源 revision。
- `data/cache/`：缓存海报、背景图和生成的媒体资源。删除媒体库时，也会清理该库独占引用的 TMDB 图片缓存。

当前仍处于 pre-1.0 MVP 预览版阶段，schema 变更继续直接修改 `migrations/0001_init.sql`。当前 schema 包含 realtime、后台任务、扫描检查点和通用通知表，无法平滑升级旧数据库：需要重置 `data/postgres/`、重新初始化数据库并重新扫描媒体库。

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
- SSE 同步协议: [docs/SSE.md](docs/SSE.md)
- 媒体库扫描与刮削设计: [docs/MEDIA_LIBRARY_SCAN.md](docs/MEDIA_LIBRARY_SCAN.md)
- TMDB 对接审查与目标契约: [docs/TMDB.md](docs/TMDB.md)
- 前端: [apps/mova-web/README.md](apps/mova-web/README.md)
- 后端: [apps/mova-server/README.md](apps/mova-server/README.md)
- Crates: [crates/README.md](crates/README.md)

## 路线图与反馈

Mova 仍在积极迭代中。作者也在积极维护 Pad 和 macOS 客户端方向，让它们可以更自然地接入同一个自托管媒体服务器。

欢迎提交反馈、功能建议、客户端接入想法和体验改进意见。

## 许可证

当前许可证：`AGPL-3.0-only`。详见 [LICENSE](LICENSE)。
