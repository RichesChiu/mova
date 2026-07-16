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
| `src/libraries.rs` | 媒体库配置读写与删除前 artwork 引用查询；媒体库不再持久化启用/禁用状态；元数据语言变化时还会把该库全部媒体条目标记为 `pending`，确保下一次扫描覆盖全库远端元数据。 |
| `src/users.rs` | 用户、密码、Web session、原生客户端 access/refresh token 设备会话、媒体库授权。 |
| `src/scan_jobs.rs` | 扫描任务创建、持久化 phase、任务级文件计数、幂等本地分析检查点、收尾和历史查询；入队时和 `background_jobs` 在同一事务中写入。 |
| `src/background_jobs.rs` | PostgreSQL 后台任务的 `SKIP LOCKED` 领取、租约续期、完成和延迟重试。 |
| `src/realtime.rs` | 读取稳定 `server_epoch`、资源 revisions 和当前用户可见的活跃扫描。 |
| `src/playback_progress.rs` | 维护逐文件播放进度，并同步维护最多 20 部电影或 Series 的活跃 Continue 队列。 |
| `src/media_cast.rs` | 演员成员表与演员同步记录表读写；详情页按需补全后会直接持久化到这里。 |
| `src/media_items.rs` | 媒体条目相关父模块。 |
| `src/media_items/query.rs` | 媒体列表、详情、文件、音轨、季集、outline cache 等读侧查询，也负责按 `file_path` 读取既有 metadata 摘要、`metadata_status` 复核状态、`scan_hash` 和 `local_analysis_version`；复用本地分析时音轨与字幕按 media file ID 集合各执行一次批量查询。 |
| `src/media_items/sync.rs` | 按路径 upsert / delete 媒体项、媒体文件、音轨和字幕轨道，并在文件删除或重归属时清理孤立条目；同一 TMDB `provider_item_id` 的电影本地版本会在这里复用同一个 `movie media_item`。扫描组使用单个短事务写入全部成员，任一成员失败时整组回滚，每个组只执行一次孤儿结构清理，并在延迟逐行 trigger 后显式增加一次 catalog revision。 |
| `src/media_items/series.rs` | 剧集聚合写入与 `series / seasons / episodes` 相关持久化；同一季同一集的多个文件版本会复用同一个 episode 记录，并把剧集级 metadata 复核状态写在 series media item 上；扫描 pending / unmatched 阶段不会用空 artwork 覆盖已有剧集、季或集图片，只有 matched 元数据写入能确认清空缺失图片。 |

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
- `list_library_details`
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
- `create_native_client_session`
- `get_user_by_native_access_token_hash`
- `get_native_client_session_by_refresh_token_hash`
- `get_used_native_refresh_token`
- `touch_native_client_session`
- `rotate_native_client_session_tokens`
- `revoke_native_client_session`
- `revoke_native_client_session_by_refresh_token_hash`
- `revoke_native_client_sessions_for_user`
- `update_user_password`

### 扫描任务

- `enqueue_scan_job`
- `create_scan_job`
- `mark_scan_job_running`
- `update_scan_job_phase`
- `update_scan_job_progress`
- `initialize_scan_job_work`
- `mark_scan_group_analyzed`
- `record_scan_job_attempt_failure`
- `mark_scan_job_retry_pending`
- `finalize_scan_job`
- `list_scan_jobs_for_library`
- `get_scan_job`
- `get_latest_scan_job_for_library`

`scan_jobs.progress_percent` 保存任务级权威进度。`local_analyzed_files`、`local_committed_files`、`remote_completed_files` 通过 `(scan_job_id, group_key)` 检查点幂等推进，公式为 `floor(10 + 20×analyzed/total + 20×committed/total + 49×remote/total)`。运行中取当前值与新值的较大值并限制到 99，只有 `finalize_scan_job` 写入成功终态时才设置为 100；失败或取消保留最后完成的进度，供重连后的 active scan 和历史任务接口恢复。

可重试执行失败先由 `record_scan_job_attempt_failure` 保存错误上下文；后台任务仍有额度时由 `mark_scan_job_retry_pending` 把父任务恢复为 `pending`，不提前写终态。只有重试耗尽后才调用 `finalize_scan_job(..., "failed", ...)`。

### 后台任务

- `claim_background_job`
- `renew_background_job_lease`
- `complete_background_job`
- `retry_or_fail_background_job`

### Realtime 状态

- `get_realtime_server_epoch`
- `list_realtime_revisions`
- `list_active_scan_jobs`

### 媒体浏览与同步

- `list_media_items_for_library`
- `list_media_item_previews_by_library`
- `list_recently_added_media_items_by_library`
- `get_media_item`
- `get_media_file`
- `get_audio_track`
- `get_season`
- `list_existing_media_metadata_for_file_paths`
- `list_media_files_for_media_item`
- `list_audio_tracks_for_media_file`
- `list_seasons_for_series`
- `list_episodes_for_season`
- `sync_library_media`
- `sync_library_media_best_effort`
- `upsert_library_media_entry_by_file_path`
- `upsert_library_media_entries_by_file_path`
- `list_audio_tracks_for_media_files`
- `list_subtitle_files_for_media_files`
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

## 6. 当前数据访问风格

这个 crate 当前采用的是：

- 每个业务能力一个模块
- 每个函数直接写 `sqlx::query(...)`
- 在函数内把 `PgRow` 映射成 `mova-domain`

也就是说，这里没有额外的 repository trait 抽象层；当前策略是保持 SQL 显式、直接、容易追踪。

## 7. 当前值得注意的点

- 仓库仍处于 pre-1.0 阶段，用户确认正式 MVP 前 migration 保持在根目录 [`../../migrations/0001_init.sql`](../../migrations/0001_init.sql)。
- 这个阶段 schema 变更默认要求重建数据库 / 重置数据目录，不新增后续 migration 兼容旧库；当前 schema 包含 `background_jobs`、`realtime_system_state` 和 `realtime_revisions`，旧开发库不能平滑升级，需要重置后重新扫描。
- 业务表 mutation trigger 会在同一事务内调用 `mova_bump_realtime_revision` 并发送 PostgreSQL `NOTIFY`；revision 是可靠状态，NOTIFY 只用于唤醒 dispatcher。
- `mova-server` 启动时会直接调用这里的 `connect / migrate / ping`，未完成后台任务通过租约过期后重新领取，不再在启动时批量标记失败。
- `mova-application` 的大部分业务用例都会在这里落到最终 SQL。

如果要看谁在调用这些持久层函数：

- 应用层：[`../mova-application/README.md`](../mova-application/README.md)
- 服务端：[`../../apps/mova-server/README.md`](../../apps/mova-server/README.md)
