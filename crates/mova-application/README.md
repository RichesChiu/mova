# mova-application

`mova-application` 是 Mova 的应用层 crate。  
它承接“业务用例”本身，不直接暴露 HTTP，也不负责 SQL 细节；通常由 `mova-server` 的 handler 调用，再下沉到 `mova-db` 和 `mova-scan`。

## 1. 这个 crate 在系统里的位置

调用关系通常是：

`mova-server handlers` -> `mova-application` -> `mova-db` / `mova-scan`

它的职责是：

- 组织业务用例
- 做参数归一化和业务校验
- 编排扫描、元数据补全和播放进度流程
- 组合多个持久层/扫描层能力
- 向上层导出稳定的应用层 API

它不负责：

- Axum 路由和 HTTP 协议
- Cookie / session header 等传输层细节
- 原始 SQL

## 2. 入口文件

| 文件 | 作用 |
| --- | --- |
| `src/lib.rs` | crate 入口。负责声明模块，并把各业务模块对外需要的函数、输入结构和输出结构统一 `pub use` 出去。 |
| `src/error.rs` | 应用层统一错误类型 `ApplicationError` 和 `ApplicationResult`。 |

`mova-server` 基本只依赖 `lib.rs` 导出的这些函数，而不是直接碰内部模块。

## 3. 依赖

### 直接依赖的 workspace crate

- `mova-db`
- `mova-domain`
- `mova-scan`

### 主要外部依赖

- `reqwest`
- `sqlx`
- `tokio`
- `serde` / `serde_json`
- `argon2`
- `async-trait`
- `tracing`

这些依赖分别用于：

- 调用远端元数据服务
- 访问数据库返回值
- 扫描和后台任务编排
- 密码哈希与认证
- provider trait 抽象

## 4. 当前模块

| 文件 | 作用 |
| --- | --- |
| `src/libraries.rs` | 媒体库创建、更新、删除、详情与列表。 |
| `src/users.rs` | 用户创建、编辑、删除、登录、登出、bootstrap、昵称更新、密码修改、库授权。 |
| `src/scan_jobs.rs` | 扫描任务入队、执行、进度事件、取消态和任务查询。 |
| `src/file_sync.rs` | watcher / reconcile 触发后的路径级同步与库存对齐。 |
| `src/media_items.rs` | 媒体条目详情、列表、文件、音轨、剧集 outline、季集查询、元数据刷新。 |
| `src/media_enrichment.rs` | 扫描过程中对单条媒体做 TMDB / sidecar / 图片补全，并在远端失败时回退到本地解析结果。 |
| `src/metadata.rs` | 元数据 provider 抽象、TMDB client、可选 OMDb IMDb 评分补齐、国家/地区/题材类型/工作室补齐、语言归一化、远端请求超时，以及“年份先过滤、失败再去年份”的软匹配策略。 |
| `src/metadata_match.rs` | 管理员手动搜索候选元数据并应用匹配。 |
| `src/media_cast.rs` | 演员列表查询与缓存失效。 |
| `src/media_classification.rs` | 媒体库类型和电影/剧集归类辅助逻辑。 |
| `src/playback_header.rs` | 播放器页头部信息查询。 |
| `src/playback_progress.rs` | 单条播放进度、继续观看和播放进度写入。 |
| `src/watch_history.rs` | 当前用户观看历史查询。 |

## 5. 主要导出能力

`src/lib.rs` 当前按业务分组导出这些能力：

### 媒体库

- `create_library`
- `update_library`
- `delete_library`
- `list_libraries`
- `get_library`
- `get_library_detail`

### 用户与认证

- `bootstrap_required`
- `bootstrap_admin`
- `login`
- `logout`
- `get_user_by_session_token`
- `update_own_profile`
- `change_own_password`
- `create_user`
- `update_user`
- `delete_user`
- `replace_user_library_access`
- `reset_user_password`

### 扫描与同步

- `enqueue_library_scan`
- `execute_scan_job`
- `execute_scan_job_with_cancellation`
- `list_scan_jobs_for_library`
- `get_scan_job_for_library`
- `reconcile_library_inventory`
- `sync_library_filesystem_changes`

### 媒体浏览与元数据

- `get_media_item`
- `list_media_items_for_library`
- `list_media_files_for_media_item`
- `list_audio_tracks_for_media_file`
- `list_seasons_for_series`
- `list_episodes_for_season`
- `series_episode_outline_for_media_item`
- `get_audio_track`
- `refresh_media_item_metadata`
- `search_media_item_metadata_matches`
- `apply_media_item_metadata_match`
- `list_media_item_cast`

### 播放

- `get_media_item_playback_header`
- `list_audio_tracks_for_media_file`
- `get_audio_track`
- `get_playback_progress_for_media_item`
- `update_playback_progress_for_media_item`
- `list_continue_watching`
- `list_watch_history`

## 6. 当前最关键的几条业务链

### 建库

`create_library` / `update_library`

- 归一化名称、描述、元数据语言
- 校验 `root_path`
- 再调用 `mova-db` 落库

### 扫描

`enqueue_library_scan` -> `execute_scan_job_with_cancellation`

- 先在数据库里创建/复用扫描任务
- 调用 `mova-scan` 发现媒体文件
- 先把电影文件或剧集目录组归成更接近用户理解的扫描展示单位
- 对剧集优先使用目录名做组级元数据匹配，再补图片和本地季集结构
- 再调用 `mova-db` 做媒体同步；如果整批写入被单条脏数据卡住，会回退到逐条 best-effort 写入，尽量保住其余正常条目
- 过程中持续发出 `ScanJobEvent`

### 手动元数据匹配

`search_media_item_metadata_matches` -> `apply_media_item_metadata_match`

- 先基于当前媒体项构造搜索条件
- 让 provider 返回候选项
- 选中结果后覆盖本地元数据
- 同时失效演员和剧集大纲相关缓存

### 播放进度

`update_playback_progress_for_media_item`

- 按用户维度更新 `playback_progress`
- 同步维护 `watch_history`
- 保证“继续观看”和“历史记录”两条读链可以复用这份状态

## 7. 适合在这里继续放什么

适合继续放进 `mova-application` 的：

- 业务用例
- 多模块编排
- 输入归一化与业务校验
- 对外稳定导出的应用层函数

不适合继续放进来的：

- Axum handler
- 纯 SQL
- 只服务于某一个 HTTP response 的 DTO

如果要看接口和服务端调用它的方式：

- 服务端入口：[`../../apps/mova-server/README.md`](../../apps/mova-server/README.md)
- API 契约：[`../../docs/API.md`](../../docs/API.md)
