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
| `src/file_sync.rs` | 手动扫库或显式路径同步时的库存对齐与增量写入。 |
| `src/intro_detection.rs` | 剧集片头按需检测；只在播放某一集且当前季/当前集还没有片头数据时调用 Python 脚本做分析。 |
| `src/media_items.rs` | 媒体条目详情、列表、文件、音轨、剧集 outline、季集查询、元数据刷新。 |
| `src/media_enrichment.rs` | 扫描过程中按本地聚合组做 TMDB / sidecar / 图片补全，并在远端失败时保留本地解析结果。 |
| `src/metadata.rs` | 元数据 provider 抽象、TMDB client、可选 OMDb IMDb 评分补齐、国家/地区/题材类型/工作室补齐、语言归一化、远端请求超时，以及“年份先过滤、失败再去年份”的软匹配策略。 |
| `src/metadata_match.rs` | 管理员手动搜索候选元数据并应用匹配。 |
| `src/media_cast.rs` | 演员列表查询与按需持久化同步；详情页首次需要演员信息时才会拉远端并写库。 |
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
- 调用 `mova-scan` 做轻量文件清单发现，只读取路径、大小和修改时间
- 用同路径 `media_files.scan_hash` 和 `media_files.local_analysis_version` 判断是否能跳过本地分析；已经成功匹配、文件指纹未变化、本地分析版本未变化、已有 TMDB 绑定且已有本地可见海报的路径，不会重新跑拆名、sidecar、`ffprobe`、TMDB / OMDb、图片缓存或数据库 upsert
- 对新增、变化、本地分析版本过期的路径先调用 `mova-scan::inspect_media_file_inventory_shallow` 做浅层文件名 / 路径解析，不读取 sidecar、不调用 `ffprobe`，只用来建立稳定的电影或剧集扫描组，避免前端先看到 `A.S01E01` 这类临时错误卡片
- 对文件指纹和本地分析版本都未变化，但 `unmatched`、`failed`、从未成功绑定 TMDB、缺少可见海报、仍保留远端图片 URL，或已绑定 TMDB 但展示名仍等于本地带年份占位名的路径，浅层聚合仍只看当前文件名 / 路径；进入组内完整分析时可直接从数据库恢复上次本地分析结果，跳过拆名、sidecar、`ffprobe`，只进入后续 TMDB 补全
- 浅层聚合完成后，服务端按扫描组逐个调用完整本地分析：读取 sidecar、调用 `ffprobe`、补音轨字幕和技术标签；每个组完成后立即写入数据库并推送 `scan.item.updated stage=discovered`，然后才继续处理下一组
- 远端补全阶段才按本地聚合组串行访问 TMDB 做类型确认、metadata、海报和本地图片缓存；同一剧集组只做一次剧集 metadata 查询，再把 TMDB 标题和剧集级海报应用到组内所有集；成功后标记 `metadata_status = matched` 并立即覆盖写库，失败或从未成功访问过 TMDB 的条目会在后续手动扫描中重试
- 对剧集会从文件名里的 `SxxExx` 先拆出剧名和年份做组级元数据匹配；文件名里的年份只作为匹配提示，不作为剧集身份键，所以 `The Boys (2019) - S01E01` 和 `The Boys (2020) - S02E01` 会聚合到同一剧集；`The.BeautyS01E01` 这类标题后直接跟 `SxxExx` 的文件名也会拆出剧名和季集号
- 如果文件位于明确的季目录树下，会用共同的剧集容器目录做扫描展示聚合；这能把同一剧集文件夹内不同季、不同语言文件名的资源先合成一个剧集单位
- TMDB 补全成功前，扫描占位和本地入库条目使用本地分析出的电影或剧集名称；TMDB 补全成功后，展示标题必须使用 TMDB 返回的名称覆盖本地名称，后续本地剧集归组只更新 `source_title` / 季集结构，不要让目录名或本地解析名压住远端结果
- 没有本地季集号的文件会先做 TMDB movie / tv 类型确认；只有远端明确匹配电影时才绑定电影 metadata，远端更像剧集但本地没有季集号、或远端匹配失败时，会写入 `metadata_status = unmatched/failed` 和明确失败原因，进入前端 Other 复核区
- TMDB 未启用时写入 `metadata_status = skipped`，不把它当作刮削失败，前端仍按本地 `media_type` 展示
- 每个扫描展示组会先以本地分析结果 upsert 一次；完成 metadata / 海报后会再次调用 `mova-db` 覆盖该组文件，并发出带 `poster_path` / `overview` / `metadata_status` 的 `ScanJobEvent::ItemUpdated`
- 最后只对缺失路径做删除 reconcile；未变化路径完全保留，不参与重探测和 upsert

### 片头检测

`get_media_item_playback_header`

- 电影直接返回播放器页头部信息
- 剧集会先检查当前集和所在季是否已经有片头区间
- 只有在当前播放资源缺少片头数据时，才会触发一次 season 级按需检测
- Python 脚本内部会自行调用 `ffmpeg` 做音频提取，再把 season 级 `intro_start_seconds` / `intro_end_seconds` 回写数据库
- 检测失败不会阻断播放，只是这次先继续按“无片头数据”处理

### 手动元数据匹配

`search_media_item_metadata_matches` -> `apply_media_item_metadata_match`

- 先基于当前媒体项构造搜索条件
- 让 provider 返回候选项
- 选中结果后覆盖本地元数据
- 同时失效演员和剧集大纲相关缓存

### 演员信息

`list_media_item_cast`

- 电影和剧集详情页请求演员时，先读本地已持久化的演员数据
- 只有在本地还没有演员信息时，才会按需拉一次远端演员并直接写库
- 一旦写入成功，后续详情页默认直接复用，不再按 TTL 自动刷新
- 手动 metadata 匹配或手动刷新 metadata 后，会清掉旧演员数据并按新条目重新同步

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
