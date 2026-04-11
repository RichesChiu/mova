# mova-db

`mova-db` 是 Mova 的持久层 crate。  
它负责 PostgreSQL 连接、migration、原始 SQL 查询和数据库结果到领域对象的映射。

## 1. 这个 crate 在系统里的位置

调用关系通常是：

`mova-server` 启动阶段 / `mova-application` 用例层 -> `mova-db`

它的职责是：

- 数据库连接和 migration
- 所有 SQL 查询与更新
- 把行数据映射成 `mova-domain` 对象
- 提供最小、可复用的持久层 API

它不负责：

- HTTP 协议
- 业务流程编排
- 文件系统扫描

## 2. 入口文件

| 文件 | 作用 |
| --- | --- |
| `src/lib.rs` | crate 入口，聚合并导出所有持久层函数。 |
| `src/pool.rs` | 数据库配置、连接、migration、`ping`。 |

## 3. 依赖

### 直接依赖的 workspace crate

- `mova-domain`

### 主要外部依赖

- `sqlx`
- `time`
- `anyhow`

## 4. 当前模块

| 文件 | 作用 |
| --- | --- |
| `src/pool.rs` | `DatabaseSettings`、`connect`、`migrate`、`ping`。 |
| `src/libraries.rs` | 媒体库 CRUD。 |
| `src/users.rs` | 用户、密码、session、媒体库授权。 |
| `src/scan_jobs.rs` | 扫描任务创建、运行态更新、收尾、历史查询。 |
| `src/playback_progress.rs` | 播放进度与继续观看相关表读写。 |
| `src/watch_history.rs` | 观看历史表读写。 |
| `src/media_cast.rs` | 演员缓存表读写。 |
| `src/media_items.rs` | 媒体条目相关父模块。 |
| `src/media_items/query.rs` | 媒体列表、详情、文件、音轨、季集、outline cache 等读侧查询。 |
| `src/media_items/sync.rs` | 按路径 upsert / delete 媒体项、媒体文件、音轨和字幕轨道，并在文件删除或重归属时清理孤立条目。 |
| `src/media_items/series.rs` | 剧集聚合写入与 `series / seasons / episodes` 相关持久化；同一季同一集的多个文件版本会复用同一个 episode 记录。 |

## 5. 主要导出能力

### 连接与初始化

- `connect`
- `migrate`
- `ping`
- `DatabaseSettings`

### 媒体库

- `create_library`
- `update_library`
- `delete_library`
- `list_libraries`
- `get_library`

### 用户与会话

- `create_user`
- `update_user`
- `delete_user`
- `get_user`
- `get_user_by_username`
- `get_user_by_session_token`
- `create_session`
- `delete_session`
- `delete_sessions_for_user`
- `replace_user_library_access`
- `update_user_password`

### 扫描任务

- `enqueue_scan_job`
- `create_scan_job`
- `mark_scan_job_running`
- `update_scan_job_progress`
- `finalize_scan_job`
- `list_scan_jobs_for_library`
- `get_scan_job`
- `get_latest_scan_job_for_library`
- `fail_incomplete_scan_jobs`

### 媒体浏览与同步

- `list_media_items_for_library`
- `get_media_item`
- `get_media_file`
- `get_audio_track`
- `get_season`
- `list_media_files_for_media_item`
- `list_audio_tracks_for_media_file`
- `list_seasons_for_series`
- `list_episodes_for_season`
- `sync_library_media`
- `upsert_library_media_entry_by_file_path`
- `delete_library_media_by_file_path`
- `delete_library_media_by_path_prefix`
- `replace_audio_tracks_for_media_file`
- `replace_subtitle_files_for_media_file`
- `update_media_item_metadata`
- `update_media_file_metadata`

### 播放

- `get_playback_progress_for_media_item`
- `list_continue_watching`
- `upsert_playback_progress`
- `create_watch_history`
- `update_watch_history`
- `list_watch_history`

## 6. 当前数据访问风格

这个 crate 当前采用的是：

- 每个业务能力一个模块
- 每个函数直接写 `sqlx::query(...)`
- 在函数内把 `PgRow` 映射成 `mova-domain`

也就是说，这里没有额外的 repository trait 抽象层；当前策略是保持 SQL 显式、直接、容易追踪。

## 7. 当前值得注意的点

- 仓库仍处于 pre-1.0 阶段，migration 目前集中在根目录 [`../../migrations/`](../../migrations/)。
- `mova-server` 启动时会直接调用这里的 `connect / migrate / ping / fail_incomplete_scan_jobs`。
- `mova-application` 的大部分业务用例都会在这里落到最终 SQL。

如果要看谁在调用这些持久层函数：

- 应用层：[`../mova-application/README.md`](../mova-application/README.md)
- 服务端：[`../../apps/mova-server/README.md`](../../apps/mova-server/README.md)
