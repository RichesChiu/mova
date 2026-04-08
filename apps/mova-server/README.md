# mova-server

`mova-server` 是 Mova 的后端进程，基于 Axum + Tokio。  
它的职责不是承载全部业务实现，而是把 HTTP/SSE 请求接进来，做鉴权和协议转换，再把真正的业务分发到 `mova-application`、`mova-db`、`mova-domain` 以及本地运行时模块。

如果你要看接口字段和响应格式，优先看 [`../../docs/API.md`](../../docs/API.md)。  
这份 README 更关注代码入口、路由结构、handler 调用链和依赖的 crate。

## 1. 启动入口与进程链路

### 1.1 入口文件

| 文件 | 作用 |
| --- | --- |
| `src/main.rs` | 进程入口。负责加载环境变量、初始化 tracing、连接数据库、执行 migration、恢复中断扫描、准备缓存目录、创建 `AppState`、初始化 watcher/runtime，并启动 Axum 服务。 |
| `src/config.rs` | 读取 `MOVA_HTTP_HOST`、`MOVA_HTTP_PORT`、`MOVA_TIMEZONE`、`MOVA_CACHE_DIR`、`MOVA_WEB_DIST_DIR` 等运行时配置。 |
| `src/metadata_provider_config.rs` | 解析元数据 provider 相关环境变量，并交给 `mova-application` 构建具体 provider。 |
| `src/app.rs` | 组装顶层 `Router`，把所有子路由统一挂到 `/api` 下，并在有前端构建产物时托管静态文件。 |
| `src/state.rs` | 定义 `AppState`、进程内扫描注册表、watcher 注册表以及 SSE 事件总线。 |
| `src/auth.rs` | 公用鉴权与访问控制助手，包括 session cookie、`require_user`、`require_admin`、媒体库/媒体项/媒体文件访问校验。 |
| `src/sync_runtime.rs` | watcher、后台路径校准、后台扫描入队与扫描任务执行的运行时逻辑。 |
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
11. `sync_runtime::initialize_library_sync()` 为已启用媒体库启动 watcher 与后台校准
12. `app::build_router()` 组装路由
13. `axum::serve()` 开始监听 HTTP

## 2. 当前后端架构

可以把当前服务端理解成下面这条调用链：

`routes/*.rs` -> `handlers/*.rs` -> `auth.rs` / `response.rs` / `state.rs` -> `mova-application` -> `mova-db`

只有少数场景会跳过 `mova-application` 直接做协议层或本地文件处理：

- 健康检查直接调用 `mova_db::ping`
- 媒体文件流直接读取磁盘并处理 `Range`
- 字幕流直接做本地缓存和 `ffmpeg` 转换
- `/server/media-tree` 直接读取容器内 `/media` 目录树
- `/events` 直接从 `RealtimeHub` 订阅 SSE 事件

### 2.1 目录职责

| 目录/文件 | 角色 |
| --- | --- |
| `routes/` | 只负责注册路径、HTTP 方法和 handler 绑定。 |
| `handlers/` | 协议层。负责请求体解析、鉴权、调用业务层、组装 response DTO。 |
| `auth.rs` | 会话与访问控制助手。 |
| `state.rs` | 进程内共享依赖和运行时注册表。 |
| `sync_runtime.rs` | 文件 watcher、后台 reconcile、扫描入队和扫描执行。 |
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
| `notify` | 文件系统 watcher。 |
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
- `library_sync_registry: LibrarySyncRegistry`
- `realtime_hub: RealtimeHub`

其中两个注册表很重要：

- `ScanRegistry`
  - 跟踪当前活跃扫描
  - 跟踪“正在删除”的媒体库
  - 提供取消扫描和等待扫描结束的能力

- `LibrarySyncRegistry`
  - 跟踪 watcher 生命周期
  - 跟踪“脏库”与正在执行的后台校准

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

### `src/sync_runtime.rs`

这是当前后端里最重要的运行时模块之一，主要负责：

- `initialize_library_sync()`：服务启动后为所有启用库恢复 watcher
- `start_library_watcher()`：为单个库启动 watcher 和后台校准
- `maybe_enqueue_initial_library_scan()`：建库/重新启用时自动补一轮扫描
- `enqueue_background_scan()`：后台扫描入队
- `spawn_library_scan_job()`：真正启动扫描执行，并把扫描事件转成 SSE 广播
- `handle_scan_registration_rejected()`：扫描注册冲突或删库冲突时做兜底收尾

### `src/realtime.rs`

`RealtimeHub` 使用 `tokio::sync::broadcast` 维护一个进程内 SSE 事件总线。  
当前事件类型包括：

- `scan.job.updated`
- `scan.job.finished`
- `scan.item.updated`
- `library.updated`
- `library.deleted`
- `media_item.metadata.updated`

所有事件都会先经过 `is_visible_to(&UserProfile)` 做库级可见性过滤，再转换成 SSE。

## 5. 路由总览

当前后端有 12 个 route module，都由 `app.rs` 合并后统一挂到 `/api` 下：

- `routes/health.rs`
- `routes/auth.rs`
- `routes/libraries.rs`
- `routes/server.rs`
- `routes/realtime.rs`
- `routes/media_files.rs`
- `routes/subtitle_files.rs`
- `routes/media_items.rs`
- `routes/seasons.rs`
- `routes/playback_progress.rs`
- `routes/users.rs`
- `routes/watch_history.rs`

如果 `config.web_dist_dir` 存在，`app.rs` 还会把前端构建产物作为 fallback 静态文件托管。

## 6. 路由模块与调用链

下面的表格重点说明：

- 路由路径
- 对应 handler
- 主要依赖的 crate / 方法
- 这个接口承担的职责

### 6.1 健康检查

#### `routes/health.rs`

| Method | Path | Handler | 主要依赖 | 作用 |
| --- | --- | --- | --- | --- |
| `GET` | `/api/health` | `handlers::health::health` | `mova_db::ping` | 检查 API 进程和数据库是否可用。 |

### 6.2 认证与会话

#### `routes/auth.rs`

| Method | Path | Handler | 主要依赖 | 作用 |
| --- | --- | --- | --- | --- |
| `GET` | `/api/auth/bootstrap-status` | `handlers::auth::get_bootstrap_status` | `mova_application::bootstrap_required` | 判断系统是否还没有管理员。 |
| `POST` | `/api/auth/bootstrap-admin` | `handlers::auth::bootstrap_admin` | `mova_application::bootstrap_admin`、`auth::attach_session_cookie` | 初始化首个管理员并立即写入 session cookie。 |
| `POST` | `/api/auth/login` | `handlers::auth::login` | `mova_application::login`、`auth::attach_session_cookie` | 用户登录并建立 session。 |
| `POST` | `/api/auth/logout` | `handlers::auth::logout` | `mova_application::logout`、`auth::clear_session_cookie` | 注销当前 session。 |
| `GET` | `/api/auth/me` | `handlers::auth::current_user` | `auth::require_user` | 返回当前登录用户。 |
| `PUT` | `/api/auth/password` | `handlers::auth::change_password` | `auth::require_user`、`mova_application::change_own_password`、`auth::attach_session_cookie` | 当前用户修改自己的密码，并轮换 session。 |

### 6.3 用户管理

#### `routes/users.rs`

| Method | Path | Handler | 主要依赖 | 作用 |
| --- | --- | --- | --- | --- |
| `GET` | `/api/users` | `handlers::users::list_users` | `auth::require_admin`、`mova_application::list_users` | 管理员查询用户列表。 |
| `POST` | `/api/users` | `handlers::users::create_user` | `auth::require_admin`、`mova_application::create_user` | 管理员创建用户。 |
| `PATCH` | `/api/users/{id}` | `handlers::users::update_user` | `auth::require_admin`、`mova_application::update_user` | 更新用户角色、启停状态等基础信息。 |
| `DELETE` | `/api/users/{id}` | `handlers::users::delete_user` | `auth::require_admin`、`mova_application::delete_user` | 删除用户。 |
| `PUT` | `/api/users/{id}/library-access` | `handlers::users::update_user_library_access` | `auth::require_admin`、`mova_application::replace_user_library_access` | 更新普通用户的媒体库授权范围。 |
| `PUT` | `/api/users/{id}/password` | `handlers::users::reset_user_password` | `auth::require_admin`、`mova_application::reset_user_password` | 管理员重置指定用户密码。 |

### 6.4 媒体库管理与扫描

#### `routes/libraries.rs`

| Method | Path | Handler | 主要依赖 | 作用 |
| --- | --- | --- | --- | --- |
| `GET` | `/api/libraries` | `handlers::libraries::list_libraries` | `auth::require_user`、`mova_application::list_libraries` | 返回当前用户有权限看到的媒体库列表。 |
| `POST` | `/api/libraries` | `handlers::libraries::create_library` | `auth::require_admin`、`mova_application::create_library`、`sync_runtime::maybe_enqueue_initial_library_scan`、`sync_runtime::start_library_watcher`、`RealtimeEvent::LibraryUpdated` | 创建媒体库；如果启用则立即挂 watcher 并触发首轮扫描。 |
| `GET` | `/api/libraries/{id}` | `handlers::libraries::get_library` | `auth::require_library_access`、`mova_application::get_library_detail` | 查询单个媒体库详情和最近扫描摘要。 |
| `PATCH` | `/api/libraries/{id}` | `handlers::libraries::update_library` | `auth::require_admin`、`mova_application::get_library`、`mova_application::update_library`、`state.scan_registry`、`state.library_sync_registry`、`sync_runtime::start_library_watcher`、`sync_runtime::maybe_enqueue_initial_library_scan`、`RealtimeEvent::LibraryUpdated` | 更新媒体库名称、描述、元数据语言和启停状态。 |
| `DELETE` | `/api/libraries/{id}` | `handlers::libraries::delete_library` | `auth::require_admin`、`state.scan_registry`、`mova_application::delete_library`、`state.library_sync_registry.clear_library`、`RealtimeEvent::LibraryDeleted` | 删除媒体库，并先安全停止相关扫描与 watcher。 |
| `GET` | `/api/libraries/{id}/media-items` | `handlers::libraries::list_library_media_items` | `auth::require_library_access`、`mova_application::list_media_items_for_library` | 查询某个库下的媒体条目列表。 |
| `GET` | `/api/libraries/{id}/scan-jobs` | `handlers::libraries::list_library_scan_jobs` | `auth::require_admin`、`mova_application::list_scan_jobs_for_library` | 查询该库扫描历史。 |
| `GET` | `/api/libraries/{id}/scan-jobs/{scan_job_id}` | `handlers::libraries::get_library_scan_job` | `auth::require_admin`、`mova_application::get_scan_job_for_library` | 查询单个扫描任务状态。 |
| `POST` | `/api/libraries/{id}/scan` | `handlers::libraries::scan_library` | `auth::require_admin`、`mova_application::enqueue_library_scan`、`state.scan_registry`、`sync_runtime::spawn_library_scan_job`、`sync_runtime::handle_scan_registration_rejected` | 手动触发扫描，或复用当前已有活跃扫描。 |

### 6.5 服务器运行时信息

#### `routes/server.rs`

| Method | Path | Handler | 主要依赖 | 作用 |
| --- | --- | --- | --- | --- |
| `GET` | `/api/server/media-tree` | `handlers::server::get_media_tree` | `auth::require_admin`、本地文件系统递归读取 | 返回容器内 `/media` 目录树，给前端建库时选择库根路径。 |

### 6.6 实时事件

#### `routes/realtime.rs`

| Method | Path | Handler | 主要依赖 | 作用 |
| --- | --- | --- | --- | --- |
| `GET` | `/api/events` | `handlers::realtime::events` | `auth::require_user`、`state.realtime_hub.subscribe()`、`RealtimeEvent::is_visible_to()` | 建立 SSE 长连接，把扫描、媒体库和元数据事件推给前端。 |

### 6.7 媒体条目与元数据

#### `routes/media_items.rs`

| Method | Path | Handler | 主要依赖 | 作用 |
| --- | --- | --- | --- | --- |
| `GET` | `/api/media-items/{id}` | `handlers::media_items::get_media_item` | `auth::require_media_item_access`、`mova_application::list_media_item_cast` | 查询单个媒体条目详情和演员。 |
| `GET` | `/api/media-items/{id}/playback-header` | `handlers::media_items::get_media_item_playback_header` | `auth::require_media_item_access`、`mova_application::get_media_item_playback_header` | 返回播放器页头部需要的标题、季集和关联系列信息。 |
| `GET` | `/api/media-items/{id}/files` | `handlers::media_items::list_media_item_files` | `auth::require_media_item_access`、`mova_application::list_media_files_for_media_item` | 查询条目关联的物理媒体文件。 |
| `GET` | `/api/media-items/{id}/seasons` | `handlers::media_items::list_media_item_seasons` | `auth::require_media_item_access`、`mova_application::list_seasons_for_series` | 查询剧集条目的季列表。 |
| `GET` | `/api/media-items/{id}/episode-outline` | `handlers::media_items::get_media_item_episode_outline` | `auth::require_media_item_access`、`mova_application::series_episode_outline_for_media_item` | 查询全集大纲和本地可用集。 |
| `GET` | `/api/media-items/{id}/metadata-search` | `handlers::media_items::search_media_item_metadata` | `auth::require_admin`、`auth::require_media_item_access`、`mova_application::search_media_item_metadata_matches` | 管理员手动搜索候选元数据。 |
| `POST` | `/api/media-items/{id}/metadata-match` | `handlers::media_items::apply_media_item_metadata_match` | `auth::require_admin`、`mova_application::apply_media_item_metadata_match`、`RealtimeEvent::MediaItemMetadataUpdated` | 应用管理员选中的元数据匹配结果，并广播更新。 |
| `POST` | `/api/media-items/{id}/refresh-metadata` | `handlers::media_items::refresh_media_item_metadata` | `auth::require_admin`、`mova_application::refresh_media_item_metadata`、`RealtimeEvent::MediaItemMetadataUpdated` | 手动重拉单条媒体元数据。 |
| `GET` | `/api/media-items/{id}/poster` | `handlers::media_items::get_media_item_poster` | `auth::require_media_item_access`、本地图片输出 | 返回媒体条目海报图。 |
| `GET` | `/api/media-items/{id}/backdrop` | `handlers::media_items::get_media_item_backdrop` | `auth::require_media_item_access`、本地图片输出 | 返回媒体条目背景图。 |

### 6.8 季与剧集附属资源

#### `routes/seasons.rs`

| Method | Path | Handler | 主要依赖 | 作用 |
| --- | --- | --- | --- | --- |
| `GET` | `/api/seasons/{id}/episodes` | `handlers::seasons::list_season_episodes` | `auth::require_season_access`、`mova_application::list_episodes_for_season` | 查询某一季下的集列表。 |
| `GET` | `/api/seasons/{id}/poster` | `handlers::seasons::get_season_poster` | `auth::require_season_access`、本地图片输出 | 返回季海报图。 |
| `GET` | `/api/seasons/{id}/backdrop` | `handlers::seasons::get_season_backdrop` | `auth::require_season_access`、本地图片输出 | 返回季背景图。 |

### 6.9 媒体文件与字幕流

#### `routes/media_files.rs`

| Method | Path | Handler | 主要依赖 | 作用 |
| --- | --- | --- | --- | --- |
| `GET` | `/api/media-files/{id}/stream` | `handlers::media_files::stream_media_file` | `auth::require_media_file_access`、本地文件读取、`Range` 解析、`ReaderStream` | 以流方式输出媒体文件内容。 |
| `HEAD` | `/api/media-files/{id}/stream` | `handlers::media_files::head_media_file` | `auth::require_media_file_access`、本地文件元数据读取 | 返回媒体文件响应头，不输出实体。 |
| `GET` | `/api/media-files/{id}/subtitles` | `handlers::subtitle_files::list_media_file_subtitles` | `auth::require_media_file_access`、`mova_application::list_subtitle_files_for_media_file` | 查询媒体文件可切换字幕轨道。 |

#### `routes/subtitle_files.rs`

| Method | Path | Handler | 主要依赖 | 作用 |
| --- | --- | --- | --- | --- |
| `GET` | `/api/subtitle-files/{id}/stream` | `handlers::subtitle_files::stream_subtitle_file` | `auth::require_user`、`mova_application::get_subtitle_file`、`auth::require_media_file_access`、本地缓存、`ffmpeg` 转 WebVTT | 把外挂或内嵌字幕转换成浏览器可直接挂载的 WebVTT 输出。 |

### 6.10 播放进度与观看历史

#### `routes/playback_progress.rs`

| Method | Path | Handler | 主要依赖 | 作用 |
| --- | --- | --- | --- | --- |
| `GET` | `/api/playback-progress/continue-watching` | `handlers::playback_progress::list_continue_watching` | `auth::require_user`、`mova_application::list_continue_watching` | 返回当前用户的继续观看列表。 |
| `GET` | `/api/media-items/{id}/playback-progress` | `handlers::playback_progress::get_media_item_playback_progress` | `auth::require_media_item_access`、`mova_application::get_playback_progress_for_media_item` | 查询某个媒体条目的最近播放进度。 |
| `PUT` | `/api/media-items/{id}/playback-progress` | `handlers::playback_progress::update_media_item_playback_progress` | `auth::require_media_item_access`、`mova_application::update_playback_progress_for_media_item` | 写入或更新当前用户的播放进度，并同步继续观看与观看历史。 |

#### `routes/watch_history.rs`

| Method | Path | Handler | 主要依赖 | 作用 |
| --- | --- | --- | --- | --- |
| `GET` | `/api/watch-history` | `handlers::watch_history::list_watch_history` | `auth::require_user`、`mova_application::list_watch_history` | 返回当前用户的观看历史。 |

## 7. 当前最关键的几个调用链

### 7.1 登录链路

`routes/auth.rs` -> `handlers/auth.rs` -> `mova_application::{login, bootstrap_admin, change_own_password}` -> `auth.rs` session cookie helpers

### 7.2 建库与启用链路

`routes/libraries.rs` -> `handlers::libraries::{create_library, update_library}` -> `mova_application::{create_library, update_library}` -> `sync_runtime::{start_library_watcher, maybe_enqueue_initial_library_scan}` -> `realtime.rs`

### 7.3 手动扫描链路

`routes/libraries.rs` -> `handlers::libraries::scan_library` -> `mova_application::enqueue_library_scan` -> `sync_runtime::spawn_library_scan_job` -> `mova_application::execute_scan_job_with_cancellation` -> `RealtimeHub`

### 7.4 SSE 链路

`sync_runtime.rs` / `handlers::libraries.rs` / `handlers::media_items.rs` 发布 `RealtimeEvent` -> `state.realtime_hub` -> `handlers::realtime::events` -> 浏览器 `EventSource`

### 7.5 播放链路

播放器先请求：

- `/api/media-items/{id}/playback-header`
- `/api/media-items/{id}/files`
- `/api/media-files/{id}/subtitles`
- `/api/media-items/{id}/playback-progress`

真正播放时：

- `/api/media-files/{id}/stream` 直接读取磁盘文件
- `/api/subtitle-files/{id}/stream` 负责把字幕转换成 WebVTT
- `/api/media-items/{id}/playback-progress` 负责轮询保存进度

## 8. 测试与验证

当前 `mova-server` 里主要有两层测试：

- 默认会跑的纯单测
  - 例如 `state.rs` 的注册表行为、`response.rs` 的映射、`realtime.rs` 的事件名与可见性
- 需要数据库环境的集成测试
  - 例如 `handlers/playback_progress.rs` 和 `handlers/realtime.rs` 里的播放进度 / SSE 契约测试
  - `handlers/realtime.rs` 现在还覆盖 SSE 可见性过滤，确认 viewer 只会收到自己有权限媒体库的事件
  - `handlers/users.rs` 里的用户 CRUD 边界测试，覆盖自禁用、自改角色、自删限制，以及禁用/删除用户后的 session 清理
  - `handlers/libraries.rs` 里的媒体库 CRUD 运行时测试，覆盖删库冲突、停用库时 watcher 停止、删库时取消活跃扫描并清理运行时状态，以及扫描相关接口仍然只允许管理员访问

运行建议：

```bash
cargo test -p mova-server
```

如果你要跑数据库集成测试，需要额外提供可访问的 `DATABASE_URL`。  
当前这类测试默认标成 `ignored`，因为它们依赖一个真实可写的 Postgres 测试库。
