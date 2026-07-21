# mova-server

`mova-server` 是 Mova 的后端进程，基于 Axum + Tokio。  
它的职责不是承载全部业务实现，而是把 HTTP/SSE 请求接进来，做鉴权和协议转换，再把真正的业务分发到 `mova-application`、`mova-db`、`mova-domain` 以及本地运行时模块。

如果你要看接口字段和响应格式，优先看 [`../../docs/API.md`](../../docs/API.md)；资源 revision、SSE 事件与跨端恢复流程见 [`../../docs/SSE.md`](../../docs/SSE.md)。
这份 README 更关注代码入口、路由结构、handler 调用链和依赖的 crate。

## 1. 启动入口与进程链路

### 1.1 入口文件

| 文件 | 作用 |
| --- | --- |
| `src/main.rs` | 进程入口。负责加载环境变量、初始化 tracing、连接数据库、执行 migration、启动 realtime dispatcher 与后台 worker 池、准备缓存目录、创建 `AppState`，并启动 Axum 服务。 |
| `src/config.rs` | 读取 `MOVA_HTTP_HOST`、`MOVA_HTTP_PORT`、`MOVA_TIMEZONE`、`MOVA_CACHE_DIR`、`MOVA_WEB_DIST_DIR`、`MOVA_WORKER_CONCURRENCY` 等运行时配置。 |
| `src/metadata_provider_config.rs` | 解析元数据 provider 相关环境变量，并交给 `mova-application` 构建具体 provider；当前会处理 TMDB token、可选的 OMDb key，以及对应的 base URL。 |
| `src/app.rs` | 组装顶层 `Router`，把所有子路由统一挂到 `/api` 下，并在有前端构建产物时托管静态文件。 |
| `src/state.rs` | 定义 `AppState`、进程内扫描租约注册表、后台任务唤醒器以及 realtime 依赖。 |
| `src/auth.rs` | 公用鉴权与访问控制助手，包括 Web session cookie、原生客户端 Bearer access token、`require_user`、`require_admin`、媒体库/媒体项/媒体文件访问校验。 |
| `src/sync_runtime.rs` | PostgreSQL 后台任务 worker 池、扫描任务领取/续租/重试和扫描执行运行时。 |
| `src/realtime.rs` | PostgreSQL revision 监听、服务端批量 dispatcher、有界 SSE 最后一跳和事件可见性过滤。 |
| `src/response.rs` | 把领域对象映射成 API response DTO，并统一包裹 JSON envelope。 |
| `src/error.rs` | 统一的 `ApiError` 和 HTTP 错误响应映射。 |

### 1.2 启动顺序

`main.rs` 当前的启动顺序是：

1. `dotenvy::dotenv()` 加载本地 `.env`
2. `init_tracing()` 初始化日志
3. `AppConfig::from_env()` 读取运行时配置
4. `mova_db::connect()` 建立数据库连接
5. `mova_db::migrate()` 执行 migration
6. `mova_db::ping()` 做数据库连通性检查
7. 创建缓存目录 `cache_dir`
8. `mova_application::build_metadata_provider()` 初始化元数据 provider
9. 启动 PostgreSQL revision listener、`RealtimeDispatcher` 和由 `MOVA_WORKER_CONCURRENCY` 控制的扫描 worker 池
10. 构建 `AppState`
11. `app::build_router()` 组装路由
12. `axum::serve()` 开始监听 HTTP

## 2. 当前后端架构

可以把当前服务端理解成下面这条调用链：

`routes/*.rs` -> `handlers/*.rs` -> `auth.rs` / `response.rs` / `state.rs` -> `mova-application` -> `mova-db`

只有少数场景会跳过 `mova-application` 直接做协议层或本地文件处理：

- 健康检查直接调用 `mova_db::ping`
- 媒体文件流直接读取磁盘并处理 `Range`
- 字幕流直接做本地缓存和 `ffmpeg` 转换
- 剧集片头自动检测会在扫描阶段调用 Python 脚本，并由脚本内部继续调用 `ffmpeg`
- `/server/media-tree` 直接读取容器内 `/media` 目录树
- `/realtime/events` 从 `RealtimeHub` 订阅已经批量合并并按权限过滤的 SSE 消息

### 2.1 目录职责

| 目录/文件 | 角色 |
| --- | --- |
| `routes/` | 只负责注册路径、HTTP 方法和 handler 绑定。 |
| `handlers/` | 协议层。负责请求体解析、鉴权、调用业务层、组装 response DTO。 |
| `auth.rs` | 会话与访问控制助手。 |
| `state.rs` | 进程内共享依赖和运行时注册表。 |
| `sync_runtime.rs` | 手动扫描入队、扫描执行和扫描事件广播。 |
| `realtime.rs` | SSE 事件总线与事件可见性过滤。 |
| `response.rs` | API 输出映射。 |
| `config.rs` | 环境变量解析。 |

## 3. 依赖的 crate 与主要作用

### Workspace 内部 crate

| crate | 在 `mova-server` 里的主要作用 |
| --- | --- |
| `mova-application` | 业务用例入口。绝大多数 handler 都调用这里的函数，而不是直接写 SQL。 |
| `mova-db` | 数据库连接、migration、健康检查，以及少数运行时兜底操作。 |
| `mova-domain` | 共享领域对象，例如 `UserProfile`、`Library`、`MediaItem`、`MediaFile`。 |

### 主要外部依赖

| 依赖 | 作用 |
| --- | --- |
| `axum` | HTTP 路由与 handler 框架。 |
| `axum-extra` | Cookie 提取与操作。 |
| `tokio` | 异步运行时、文件 IO、后台任务。 |
| `tokio-stream` | SSE 事件流、`BroadcastStream`。 |
| `tokio-util` | 媒体文件流 `ReaderStream`。 |
| `sqlx` | PostgreSQL 连接与查询。 |
| `tower-http` | 静态文件托管。 |
| `time` | API 响应时区与 session TTL。 |
| `tracing` / `tracing-subscriber` | 运行日志。 |

## 4. AppState 与运行时模块

### `src/state.rs`

`AppState` 当前包含：

- `db: PgPool`
- `api_time_offset: UtcOffset`
- `artwork_cache_dir: PathBuf`
- `metadata_provider: Arc<dyn mova_application::MetadataProvider>`
- `scan_registry: ScanRegistry`
- `realtime_hub: RealtimeHub`
- `realtime_dispatcher: RealtimeDispatcherHandle`
- `background_job_notifier: BackgroundJobNotifier`

其中两个注册表很重要：

- `ScanRegistry`
  - 跟踪当前进程实际持有租约的活跃扫描
  - 跟踪“正在删除”的媒体库
  - 提供取消扫描和等待扫描结束的能力

### `src/auth.rs`

这个文件承接所有公用访问控制逻辑，主要方法包括：

- `require_user()`
- `require_admin()`
- `request_auth_credential()`
- `require_library_access()`
- `require_media_item_access()`
- `require_media_file_access()`
- `require_season_access()`
- `attach_session_cookie()`
- `clear_session_cookie()`

也就是说，大多数 handler 不自己写权限判断，而是先经过这里。

另外，`response.rs` 现在会把本地海报/背景图映射成带版本参数的内部资源 URL；对应的图片 handler 也会回 `Cache-Control`，减少浏览页卡片反复重拉封面时的闪烁。

### `src/sync_runtime.rs`

这是当前后端里最重要的运行时模块之一，主要负责：

- HTTP handler 只把扫描任务与 `background_jobs` 记录在同一数据库事务里入队，然后唤醒 worker，不直接 `tokio::spawn` 扫描。
- worker 使用 `FOR UPDATE SKIP LOCKED` 领取任务，写入有时限的数据库租约，并在执行期间续租。
- worker 默认并发为 2，可通过 `MOVA_WORKER_CONCURRENCY` 调整；同一媒体库在进程内仍只允许一个活跃执行者。
- 执行失败按任务 attempts 和延迟重试；进程退出后未完成任务保留在 PostgreSQL，租约到期后可再次领取。
- 扫描事件交给 `RealtimeDispatcher` 合并，避免原始条目事件直接打满所有 SSE 连接。

当前这里的同步策略已经改成“首次自动扫描 + 后续手动扫描”：

- 服务端不再常驻文件 watcher
- 新建媒体库后会自动入队一次扫描
- 新增、删除、改名和移动文件都统一靠手动 `Scan Library` 收敛
- 扫描链路本身不再全量跑片头检测，避免新库首次扫库时额外吃满 CPU 和 `ffmpeg`

### `src/realtime.rs`

当前 Realtime/SSE 协议版本为 `1`。可靠实时状态保存在 PostgreSQL `realtime_revisions`，业务写入事务通过 trigger 同步增加对应 resource revision，并使用 `LISTEN/NOTIFY` 唤醒当前实例的 `RealtimeDispatcher`。SSE 不再传最终业务对象，只发送以下协议消息：

- `resources.changed`：普通资源最多每 500ms 合并；继续观看最多每 1 秒合并，标记已看完立即发送。
- `scan.progress`：按扫描任务和 `item_key` latest-wins 合并，普通进度最多每 200ms 一批；本轮存在待处理组且全部 local pending 事务提交后，会立即发送带 catalog/scan revisions 的可靠检查点。`scan_job.progress_percent` 是数据库持久化、服务端单调推进的任务级权威进度。
- `scan.finished`：与 local checkpoint 共用独立的稀疏可靠 FIFO，保证 checkpoint 先于 finished 且不受普通 Dispatcher 队列饱和影响；两个输入队列共享单调扫描事件序号，终态屏障会丢弃终态前已经排队的晚到普通事件，但允许更大序号的新一轮重试继续。单次执行失败且仍有后台重试额度时只恢复为 `pending`，不提前发送终态；最终成功、取消或重试耗尽时才立即发送 `scan.finished`，payload 同时携带最终 `catalog/scan` revisions。
- `session.invalidated`：用户权限或登录态失效后立即发送并关闭连接。

`RealtimeHub` 仍使用 `tokio::sync::broadcast`，但只作为单进程的有界最后一跳，并按 public/admin/library/user scope 分域。每条连接只订阅与自身有关的频道，用户级或单库高频事件不会再唤醒全部在线连接；管理员频道会接收所有 library scope。客户端在相关频道落后时，服务发送 `resync.required` 并关闭连接；PostgreSQL Listener 每次订阅或重订阅成功后也会要求当前连接重新对账，关闭通知丢失窗口。客户端通过 `GET /api/realtime/state` 比对持久化 revision 后恢复。扫描任务入队和持久化状态变化都会增加 `library:{id}:scan` revision，因此其他连接无需等待第一条临时进度即可发现 pending scan。`GET /api/home` 同时返回协议版本和首页快照对应的 revisions，三端可以把它作为已应用基线。

删除媒体库时，服务会先阻止新的扫描进入并等待当前扫描退出；删除事务会显式清理该库的媒体关系表、授权关系、扫描任务和播放进度。事务完成后，服务会删除该库曾引用且已经没有任何剩余记录引用的 `MOVA_CACHE_DIR/tmdb` 图片缓存文件，媒体目录里的 sidecar 图片不会被自动删除。

扫描在 `discovering` 阶段先节流发现文件、读取增量计划，再只用文件名和路径完成浅层稳定分组；这个阶段不读取 sidecar、不调用 `ffprobe`、不访问 TMDB。三项都完成后强制写入最终文件数、把任务推进到 `processing = 10`。随后一个 local worker 按组做 sidecar、`ffprobe`、音轨字幕和资源技术分析：分析状态持久化后贡献 10～30 的任务进度，pending 组事务提交后贡献 30～50，并把组放入容量为 2 的 remote channel。一个 remote worker 同时消费前面的组，访问 TMDB、缓存图片并以最终组事务推进 50～99；因此两部分有界重叠，不要求全库本地分析结束后才开始远端处理，也不保证单独显示 50。所有组终结后进入 `finalizing = 99`，任务成功提交才原子更新为 100。任务公式为 `floor(10 + 20×analyzed/total + 20×committed/total + 49×remote/total)`。

本地 pending 阶段的电影 / 剧集类型只是结构猜测，Web 会按猜测类型展示扫描卡，不会提前放入 Other。完整季集坐标只查询 TV，其它文件只查询 movie；自动候选必须严格对齐主标题。alternative title 中的 `$` 只有位于两个 ASCII 英文字母之间时才按风格化 `s` 处理，普通标题不会全局忽略空白；数字结尾的续集名可以对齐远端用明确分隔符追加的副标题。电影发行年和剧集首播年必须与远端正式年份完全一致；只导入后续季时，季年份通过 TV search `year` 和对应 season details 严格验证；没有任何年份时选择严格候选中完整日期最新者，最新日期并列时保持未匹配。对应类型没有严格命中时完成为 `unmatched / no_remote_match`，不查询另一类型兜底；TMDB 请求错误写入 `failed / metadata_provider_error`，未启用 provider 写入 `skipped`。这些终态只表示该组已完成本轮处理，因此仍计入任务完成度，但不会被误标记为匹配成功。

`MOVA_TMDB_ACCESS_TOKEN` 读取 TMDB 账户 API 设置页中的 API Read Access Token。变量缺失、为空或只含空白时，服务仍正常启动并输出一条明确 warning；本地扫描、sidecar、`ffprobe`、入库和播放继续工作，metadata enrichment 会跳过全部 TMDB 网络 lookup，最终写入 `skipped / metadata_provider_disabled`。后续配置 Token 并重启、重扫后，之前跳过且未绑定 provider 的条目会自动进入远端补全。

同一剧集组只做一次剧集 metadata 查询，再把 TMDB 标题和剧集级海报应用到组内所有集；同一 TMDB 影片下的电影资源按 `provider_item_id` 归并为一个 movie item，详情页作为多个资源版本切换。图片字段只写当前层级、当前字段自己的图，不用单集截图、其它层级图片、搜索结果图片或另一个图片字段兜底；`pending` 写库不会清空既有 artwork，只有最终 `matched` 写入才允许清理远端确认缺失的图片。临时扫描 item 会携带当前可用的 `year`、`overview`、`metadata_status` 和 `remote_media_type`，`poster_path` / `backdrop_path` 只在确认浏览器可访问时返回；Web 只在 `stage = completed` 且远端类型为空或与本地结构冲突时把扫描卡放入 Other，远端类型一致的 metadata 失败仍留在对应类型分区。`stage = completed` 后媒体写入事务会增加对应 `library:{id}:catalog` revision，客户端按资源失效通知刷新该库和轻量首页。剧集身份优先读取最近的 `tvshow.nfo`，否则使用文件名中 `SxxExx` 前的标题；目录文字不参与标题或年份候选。S01 年份是系列首播年，后续季年份只在缺少 S01 时作为对应季的 TMDB 严格验证提示。所有事件都会按 server/admin/library/user scope 做可见性过滤，再转换成 SSE。

另外，扫描现在先做轻量文件清单和同路径 `scan_hash` / `local_analysis_version` 比对；已匹配且未变化、已经有 TMDB 绑定的路径不会重新跑拆名、sidecar、`ffprobe`、TMDB / OMDb、图片缓存或数据库 upsert。新增、变更或本地分析版本过期的路径会先进入浅层聚合，再按组完整探测和 upsert。文件指纹与本地分析版本都未变化但未匹配成功、缺少 TMDB provider 绑定、旧状态是 `skipped` 但当前已启用 TMDB、按前端 Other 规则需要复核、或仍保留远端图片 URL 的路径，浅层聚合仍只看当前文件名 / 路径；进入组内完整分析时可从数据库恢复上次本地分析结果，跳过拆名、sidecar、`ffprobe`，直接进入有界 remote 流水线补 metadata/海报。自动 metadata 选择保持保守；更宽松的候选复核交给手动搜索 / 替换元数据。缺失路径会在最后统一删除。

`PATCH /api/libraries/{id}` 修改 `metadata_language` 时会先停止该库的活跃扫描，再把库内全部媒体条目标记为 `pending` 并自动入队一次全库元数据重扫。该重扫会复用未变化文件已经缓存的本地分析、音轨和字幕结果，但不会跳过任何已有媒体的远端元数据刷新。媒体库不再维护启用/禁用状态。

同名同年的严格候选有多个时，自动匹配优先保留 `original_title / original_name` 也与本地主标题严格对齐的子集；该子集仍不唯一时保持未匹配，不根据元数据语言猜测制作国家。

## 5. 路由与 feature 划分

当前后端有 13 个 route module，都由 `app.rs` 合并后统一挂到 `/api` 下：

- `routes/health.rs`
- `routes/home.rs`
- `routes/auth.rs`
- `routes/users.rs`
- `routes/libraries.rs`
- `routes/server.rs`
- `routes/realtime.rs`
- `routes/search.rs`
- `routes/media_items.rs`
- `routes/seasons.rs`
- `routes/media_files.rs`
- `routes/subtitle_files.rs`
- `routes/playback_progress.rs`

如果 `config.web_dist_dir` 存在，`app.rs` 还会把前端构建产物作为 fallback 静态文件托管。

这一层更适合按 feature 来理解，而不是在 README 里重复一整份接口文档：

- 认证与会话
  - `routes/auth.rs`
  - bootstrap、登录、登出、当前用户、当前用户改密/改昵称
- 用户管理
  - `routes/users.rs`
  - 管理员创建、更新、删除用户和重置密码；成员媒体库授权统一通过用户更新接口的 `library_ids` 字段整体替换
- 媒体库与扫描
  - `routes/libraries.rs`
  - 媒体库 CRUD、媒体条目列表、按库最新添加聚合、扫描历史、触发扫描
- 运行时与实时事件
  - `routes/server.rs`
  - `routes/realtime.rs`
  - 容器内 `/media` 目录树、持久化 realtime state、SSE 失效通知和临时扫描进度
- 媒体详情与元数据
  - `routes/media_items.rs`
  - `routes/seasons.rs`
  - 单条媒体详情、演员、统一剧集大纲、季海报背景图、手动 metadata 操作；季集层级只由 `episode-outline` 返回，演员列表会在详情页请求时按需拉取并直接写库，不再在扫库阶段预取
- 播放链路
  - `routes/media_files.rs`
  - `routes/subtitle_files.rs`
  - `routes/playback_progress.rs`
  - 文件流、音轨、字幕、继续观看、播放进度

接口路径、请求体、响应字段和权限语义统一以 [`../../docs/API.md`](../../docs/API.md) 为准。  
`mova-server/README.md` 不再重复维护逐个接口的 Method / Path 说明，只保留“这些 feature 在代码里落在哪、调用链怎么走”。

## 6. 当前最关键的几个调用链

### 6.1 登录链路

Web 端：

`routes/auth.rs` -> `handlers/auth.rs::{login, bootstrap_admin}` -> `mova_application::{login, bootstrap_admin}` -> `auth.rs` session cookie helpers

原生客户端：

`routes/auth.rs` -> `handlers/auth.rs::{token_login, refresh_token}` -> `mova_application::{login_native_client, refresh_native_client_session}` -> `response.rs::TokenLoginResponse`

原生客户端业务接口只接受 `Authorization: Bearer <access_token>`。`refresh_token` 只用于 `/auth/refresh`，成功刷新后会轮换 refresh token 并撤销旧值；用户禁用、删除或改密时会同步撤销该用户现有原生客户端会话。

### 6.2 建库与配置更新链路

`routes/libraries.rs` -> `handlers::libraries::{create_library, update_library}` -> `mova_application::{create_library, update_library}` -> PostgreSQL resource revision trigger

### 6.3 手动扫描链路

`routes/libraries.rs` -> `handlers::libraries::scan_library` -> `mova_application::enqueue_library_scan` -> `background_jobs` -> worker claim/lease -> `mova_application::execute_scan_job_with_cancellation` -> `RealtimeDispatcher`

扫描 worker 在远端组成功提交后累计任务级通知摘要；扫描结束时，任务终态、摘要 payload、`scan` 类通知和对应 realtime revisions 在同一事务提交。`GET /api/notifications` 通过 `handlers::notifications` 提供按权限过滤的通用通知、分类未读计数和已读操作。通知读取 PostgreSQL 持久化状态，不依赖 SSE 事件历史；完整的底层排障信息由 `tracing` 输出到服务日志。

### 6.4 SSE 链路

业务事务增加 `realtime_revisions` -> PostgreSQL `NOTIFY` -> `RealtimeDispatcher` 批量合并 -> `state.realtime_hub` -> `handlers::realtime::events` -> Web / macOS / iOS 客户端。断线恢复走 `handlers::realtime::state`，首页首屏走 `handlers::home::get_home`。

完整的事件触发条件、payload、客户端状态机与架构边界见 [`../../docs/SSE.md`](../../docs/SSE.md)。

### 6.5 播放链路

播放器先请求：

- `/api/media-items/{id}/playback-header`
- `/api/media-items/{id}/files`
- `/api/media-files/{id}/audio-tracks`
- `/api/media-files/{id}/subtitles`
- `/api/media-items/{id}/playback-progress`

真正播放时：

- `/api/media-files/{id}/stream` 直接读取磁盘文件
- 如果用户切到非默认音轨，会先通过 `ffmpeg -c copy` 生成一个缓存的音轨变体，再继续走同一条 `/stream` 直链与 `Range` 逻辑
- `/api/subtitle-files/{id}/stream` 负责把字幕转换成 WebVTT
- `/api/media-items/{id}/playback-progress` 负责轮询保存进度
- 如果当前剧集还没有 season / episode 级片头区间，`/api/media-items/{id}/playback-header` 会先按需触发一次 season 级片头检测；当前仍由 Python 脚本做音频比对，并把结果写进 `intro_start_seconds` / `intro_end_seconds`

## 7. 测试与验证

默认部署时，`docker-compose.yml` 直接运行已发布的 `richeschiu/mova:latest`，不会在部署机器上从源码构建镜像。本地没有镜像时，`docker compose up -d` 会自动拉取；是否升级到最新发布镜像由用户自己通过 `docker compose pull` 决定。发布镜像默认覆盖 `linux/amd64` 和 `linux/arm64`，Windows / macOS 用户通过 Docker Desktop 运行同一个 Linux 镜像，Linux 用户通过 Docker Engine / Docker Desktop 运行同一镜像。应用服务名是 `app`，容器名固定为 `mova-app`，查看日志用 `docker compose logs -f app`。需要本地源码构建时，使用 `docker-compose.build.yml` 覆盖默认服务。

源码构建时，前端阶段使用 `richeschiu/mova-web-build-base:node24-pnpm11`，Rust builder 阶段使用 `richeschiu/mova-rust-build-base:1-bookworm`，runtime 阶段使用 `richeschiu/mova-runtime-base:bookworm-ffmpeg-python3`。这些基础镜像提前内置 pnpm、Rust toolchain、ffmpeg、Python 和 runtime 证书依赖，减少本地 Docker build 时重复访问上游镜像与 apt/npm 源；对应定义集中放在 `docker/base`。发布入口是 `./scripts/publish-docker-images.sh`，脚本默认会检查基础镜像是否已经包含 `linux/amd64` 和 `linux/arm64`，缺失时先发布基础镜像，再发布主镜像。

pnpm 11 不再读取 `package.json` 里的 `pnpm` 配置字段，所以 `apps/mova-web/pnpm-workspace.yaml` 通过 `allowBuilds` 批准 `@parcel/watcher`，保证 Docker 非交互安装依赖时不会卡在 `ERR_PNPM_IGNORED_BUILDS`。

当前 `mova-server` 里主要有两层测试：

- 默认会跑的纯单测
  - 例如 `state.rs` 的注册表行为、`response.rs` 的映射、`realtime.rs` 的事件名与可见性
- 需要数据库环境的集成测试
  - 例如 `handlers/playback_progress.rs` 和 `handlers/realtime.rs` 里的播放进度 / SSE 契约测试
  - `handlers/realtime.rs` 现在还覆盖 SSE 可见性过滤，确认 viewer 只会收到自己有权限媒体库的事件
  - `handlers/users.rs` 里的用户 CRUD 边界测试，覆盖自禁用、自改角色、自删限制，以及禁用/删除用户后的 session 清理
  - `handlers/libraries.rs` 里的媒体库 CRUD 运行时测试，覆盖删库冲突、停用库时取消活跃扫描、删库时安全停止活跃扫描，以及扫描相关接口仍然只允许管理员访问

运行建议：

如果本机没有安装 Rust，当前仓库也支持直接通过 Docker 运行：

```bash
docker compose --profile tooling run --rm rust-tooling cargo test -p mova-server
```

如果本机已经安装了 Rust，也可以直接运行：

```bash
cargo test -p mova-server
```

如果你要跑数据库集成测试，需要额外提供可访问的 `DATABASE_URL`。  
当前这类测试默认标成 `ignored`，因为它们依赖一个真实可写的 Postgres 测试库。
