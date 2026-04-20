# mova-server

`mova-server` 是 Mova 的后端进程，基于 Axum + Tokio。  
它的职责不是承载全部业务实现，而是把 HTTP/SSE 请求接进来，做鉴权和协议转换，再把真正的业务分发到 `mova-application`、`mova-db`、`mova-domain` 以及本地运行时模块。

如果你要看接口字段和响应格式，优先看 [`../../docs/API.md`](../../docs/API.md)。  
这份 README 更关注代码入口、路由结构、handler 调用链和依赖的 crate。

## 1. 启动入口与进程链路

### 1.1 入口文件

| 文件 | 作用 |
| --- | --- |
| `src/main.rs` | 进程入口。负责加载环境变量、初始化 tracing、连接数据库、执行 migration、恢复中断扫描、准备缓存目录、创建 `AppState`，并启动 Axum 服务。 |
| `src/config.rs` | 读取 `MOVA_HTTP_HOST`、`MOVA_HTTP_PORT`、`MOVA_TIMEZONE`、`MOVA_CACHE_DIR`、`MOVA_WEB_DIST_DIR` 等运行时配置。 |
| `src/metadata_provider_config.rs` | 解析元数据 provider 相关环境变量，并交给 `mova-application` 构建具体 provider；当前会处理 TMDB token、可选的 OMDb key，以及对应的 base URL。 |
| `src/app.rs` | 组装顶层 `Router`，把所有子路由统一挂到 `/api` 下，并在有前端构建产物时托管静态文件。 |
| `src/state.rs` | 定义 `AppState`、进程内扫描注册表以及 SSE 事件总线。 |
| `src/auth.rs` | 公用鉴权与访问控制助手，包括 session cookie、`require_user`、`require_admin`、媒体库/媒体项/媒体文件访问校验。 |
| `src/sync_runtime.rs` | 后台扫描入队、扫描任务执行和扫描事件广播的运行时逻辑。 |
| `src/realtime.rs` | SSE 事件总线与事件枚举，负责把扫描、媒体库和元数据变更转换成 `EventSource` 可消费的数据。 |
| `src/response.rs` | 把领域对象映射成 API response DTO，并统一包裹 JSON envelope。 |
| `src/error.rs` | 统一的 `ApiError` 和 HTTP 错误响应映射。 |

### 1.2 启动顺序

`main.rs` 当前的启动顺序是：

1. `dotenvy::dotenv()` 加载本地 `.env`
2. `init_tracing()` 初始化日志
3. `AppConfig::from_env()` 读取运行时配置
4. `mova_db::connect()` 建立数据库连接
5. `mova_db::migrate()` 执行 migration
6. `mova_db::fail_incomplete_scan_jobs()` 把上次异常中断的扫描标记为失败
7. `mova_db::ping()` 做数据库连通性检查
8. 创建缓存目录 `cache_dir`
9. `mova_application::build_metadata_provider()` 初始化元数据 provider
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
- `/events` 直接从 `RealtimeHub` 订阅 SSE 事件

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

其中两个注册表很重要：

- `ScanRegistry`
  - 跟踪当前活跃扫描
  - 跟踪“正在删除”的媒体库
  - 提供取消扫描和等待扫描结束的能力

### `src/auth.rs`

这个文件承接所有公用访问控制逻辑，主要方法包括：

- `require_user()`
- `require_admin()`
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

- `enqueue_background_scan()`：后台扫描入队
- `spawn_library_scan_job()`：真正启动扫描执行，并把扫描事件转成 SSE 广播
- `handle_scan_registration_rejected()`：扫描注册冲突或删库冲突时做兜底收尾

当前这里的同步策略已经改成“首次自动扫描 + 后续手动扫描”：

- 服务端不再常驻文件 watcher
- 新建媒体库后会自动入队一次扫描
- 重新启用媒体库、新增、删除、改名和移动都统一靠手动 `Scan Library` 收敛
- 扫描链路本身不再全量跑片头检测，避免新库首次扫库时额外吃满 CPU 和 `ffmpeg`

### `src/realtime.rs`

`RealtimeHub` 使用 `tokio::sync::broadcast` 维护一个进程内 SSE 事件总线。  
当前事件类型包括：

- `scan.job.updated`
- `scan.job.finished`
- `scan.item.updated`
- `library.updated`
- `library.deleted`
- `media_item.metadata.updated`

其中 `scan.item.updated` 现在不再只是“文件级事件”：电影通常仍按文件路径发出，剧集会优先按系列目录组发出一张占位卡，避免前端在元数据拉取前先看到被打散的单集文件。所有事件都会先经过 `is_visible_to(&UserProfile)` 做库级可见性过滤，再转换成 SSE。

另外，扫描落库现在会优先尝试整库事务同步；如果因为单条脏数据导致整批写入失败，会自动回退到 best-effort 模式，尽量让其余健康条目继续写入，而不是整轮扫描直接中断。
对未改路径、且过去已经成功补过 metadata / 海报的条目，扫描在进入远端 enrichment 前还会先按 `file_path` 回填数据库里已有的 metadata 摘要，从而避免每次重扫都重复请求 TMDB / OMDb 和重新下载图片。

## 5. 路由与 feature 划分

当前后端有 12 个 route module，都由 `app.rs` 合并后统一挂到 `/api` 下：

- `routes/health.rs`
- `routes/auth.rs`
- `routes/users.rs`
- `routes/libraries.rs`
- `routes/server.rs`
- `routes/realtime.rs`
- `routes/media_items.rs`
- `routes/seasons.rs`
- `routes/media_files.rs`
- `routes/subtitle_files.rs`
- `routes/playback_progress.rs`
- `routes/watch_history.rs`

如果 `config.web_dist_dir` 存在，`app.rs` 还会把前端构建产物作为 fallback 静态文件托管。

这一层更适合按 feature 来理解，而不是在 README 里重复一整份接口文档：

- 认证与会话
  - `routes/auth.rs`
  - bootstrap、登录、登出、当前用户、当前用户改密/改昵称
- 用户管理
  - `routes/users.rs`
  - 管理员创建、更新、删除用户，重置密码，更新成员媒体库授权
- 媒体库与扫描
  - `routes/libraries.rs`
  - 媒体库 CRUD、媒体条目列表、扫描历史、触发扫描
- 运行时与实时事件
  - `routes/server.rs`
  - `routes/realtime.rs`
  - 容器内 `/media` 目录树、SSE 事件流
- 媒体详情与元数据
  - `routes/media_items.rs`
  - `routes/seasons.rs`
  - 单条媒体详情、演员、剧集大纲、季/集列表、海报背景图、手动 metadata 操作；演员列表会在详情页请求时按需拉取并直接写库，不再在扫库阶段预取
- 播放链路
  - `routes/media_files.rs`
  - `routes/subtitle_files.rs`
  - `routes/playback_progress.rs`
  - `routes/watch_history.rs`
  - 文件流、音轨、字幕、继续观看、观看历史、播放进度

接口路径、请求体、响应字段和权限语义统一以 [`../../docs/API.md`](../../docs/API.md) 为准。  
`mova-server/README.md` 不再重复维护逐个接口的 Method / Path 说明，只保留“这些 feature 在代码里落在哪、调用链怎么走”。

## 6. 当前最关键的几个调用链

### 6.1 登录链路

`routes/auth.rs` -> `handlers/auth.rs` -> `mova_application::{login, bootstrap_admin, change_own_password}` -> `auth.rs` session cookie helpers

### 6.2 建库与启用链路

`routes/libraries.rs` -> `handlers::libraries::{create_library, update_library}` -> `mova_application::{create_library, update_library}` -> `realtime.rs`

### 6.3 手动扫描链路

`routes/libraries.rs` -> `handlers::libraries::scan_library` -> `mova_application::enqueue_library_scan` -> `sync_runtime::spawn_library_scan_job` -> `mova_application::execute_scan_job_with_cancellation` -> `RealtimeHub`

### 6.4 SSE 链路

`sync_runtime.rs` / `handlers::libraries.rs` / `handlers::media_items.rs` 发布 `RealtimeEvent` -> `state.realtime_hub` -> `handlers::realtime::events` -> 浏览器 `EventSource`

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
