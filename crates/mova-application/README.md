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
- 原生客户端 access/refresh token 的 HTTP 传输细节
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
| `src/home.rs` | 通过批量数据库查询组合当前用户首页的有界快照：每库最多 16 条预览、每库最多 8 条最新添加和最多 20 条继续观看，不加载完整媒体目录，也不产生按库 N+1 查询。 |
| `src/users.rs` | 用户创建、编辑、删除、登录、登出、bootstrap、昵称更新、密码修改、库授权。 |
| `src/scan_jobs.rs` | 扫描任务入队、执行、进度事件、取消态和任务查询。 |
| `src/notifications.rs` | 按当前用户权限读取通用通知，并校验分类筛选、单条已读和批量已读用例。 |
| `src/file_sync.rs` | 手动扫库或显式路径同步时的库存对齐与增量写入。 |
| `src/intro_detection.rs` | 剧集片头按需检测；只在播放某一集且当前季/当前集还没有片头数据时调用 Python 脚本做分析。 |
| `src/media_items.rs` | 媒体条目详情、列表、文件、音轨、剧集 outline、季集查询、元数据刷新。 |
| `src/media_enrichment.rs` | 扫描过程中按本地聚合组做 TMDB / sidecar / 图片补全；远端请求错误会显式标记为 `failed`，等待后续手动扫描重试。 |
| `src/metadata.rs` | 元数据 provider 抽象、TMDB client、TMDB 评分与外部 ID 提取、国家/地区/题材类型/工作室补齐、语言归一化和远端请求超时；自动匹配由本地季集坐标选择唯一 endpoint，执行严格主标题/年份/别名验证，数字结尾续集名可以匹配明确分隔的远端副标题，无年份时选择日期最新作品。 |
| `src/metadata_match.rs` | 管理员手动搜索候选元数据并应用匹配。 |
| `src/media_cast.rs` | 演员列表查询与按需持久化同步；详情页首次需要演员信息时才会拉远端并写库。 |
| `src/media_classification.rs` | 媒体库类型和电影/剧集归类辅助逻辑。 |
| `src/playback_header.rs` | 播放器页头部信息查询。 |
| `src/playback_progress.rs` | 单条播放进度、继续观看和播放进度写入。 |

## 5. 主要导出能力

`src/lib.rs` 当前按业务分组导出这些能力：

### 媒体库

- `create_library`
- `update_library`
- `delete_library`
- `list_libraries`
- `get_library`
- `get_library_detail`

### 首页

- `get_home_snapshot`

### 用户与认证

- `bootstrap_required`
- `bootstrap_admin`
- `login`
- `login_native_client`
- `refresh_native_client_session`
- `logout`
- `logout_native_client_access_token`
- `logout_native_client_refresh_token`
- `get_user_by_session_token`
- `get_user_by_native_access_token`
- `update_own_profile`
- `change_own_password`
- `create_user`
- `update_user`
- `delete_user`
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

## 6. 当前最关键的几条业务链

### 建库

`create_library` / `update_library`

- 归一化名称、描述、元数据语言
- 校验 `root_path`
- 再调用 `mova-db` 落库

### 扫描

`enqueue_library_scan` -> `execute_scan_job_with_cancellation`

- 先在数据库里创建/复用扫描任务
- 重新扫描会先读取数据库里已经入库的 `media_files.file_path`，逐条核对真实文件是否仍存在且仍是文件；路径失效的条目会立即删除并清理关联条目
- 然后调用 `mova-scan` 做轻量文件清单发现，只读取路径、大小和修改时间，用来发现新增文件和文件指纹变化
- 用同路径 `media_files.scan_hash` 和 `media_files.local_analysis_version` 判断是否能跳过本地分析；后续只让新增/变化、本地分析版本过期、未完整匹配、或按前端 Other 规则需要复核的路径进入浅层解析、完整分析和 TMDB 补全
- 已经完整匹配、文件指纹未变化、本地分析版本未变化、且已有 TMDB 绑定的路径，不会重新跑拆名、sidecar、`ffprobe`、TMDB、图片缓存或数据库 upsert；即使 TMDB 没有可用海报，也保持稳定跳过
- 对新增、变化、本地分析版本过期的路径先调用 `mova-scan::inspect_media_file_inventory_shallow` 做浅层文件名 / 路径解析，不读取 sidecar、不调用 `ffprobe`，只用来建立稳定的电影或剧集扫描组，避免前端先看到 `A.S01E01` 这类临时错误卡片
- 本地分析版本过期时使用新规则重新拆名；只有 `matched` 且已绑定 provider ID 的条目保留旧远端展示字段，未匹配、失败或跳过条目不得用旧标题覆盖新拆名结果
- 对文件指纹和本地分析版本都未变化，但状态仍为中断遗留的 `pending`、`unmatched`、`failed`，旧状态为 `skipped` 且当前已启用 TMDB、缺少 TMDB provider 绑定、按前端 Other 规则缺少可用远端信息、仍保留远端图片 URL，或已绑定 TMDB 但展示名仍等于本地带年份占位名的路径，浅层聚合仍只看当前文件名 / 路径；进入组内完整分析时通过一次媒体摘要查询、一次批量音轨查询和一次批量字幕查询恢复上次本地分析，跳过拆名、sidecar、`ffprobe`，只进入后续 TMDB 补全
- 浅层聚合完成后，一个 local worker 按扫描组完整读取 sidecar、调用 `ffprobe`、补音轨字幕和技术标签；pending 事务提交后经容量为 2 的 channel 交给一个 remote worker，前一组访问 TMDB/缓存图片时下一组可继续本地分析，避免全库阶段屏障又保持资源有界
- 任务级 `progress_percent` 由数据库中的物理文件计数统一计算并持久化：发现完成为 10，本地分析贡献 20，pending 入库贡献 20，远端终态贡献 49，只有任务成功终结时写入 100；local/remote 可以重叠，更新始终取较大值，重排或重复事件不会让进度回退
- 扫描执行层只记录单次尝试的错误上下文；后台 worker 统一决定继续重试或写最终失败，仍有额度时父任务回到 `pending` 且不会提前发送 `scan.finished`
- 完整本地分析后的中间写库统一使用 `metadata_status = pending`；此时电影 / 剧集只代表本地结构，Web 按该结构展示扫描卡，不进入 Other。每个组完成远端匹配后才收敛到最终 metadata 状态；没有严格候选的条目进入 Other
- 远端补全阶段由完整季集坐标决定唯一 TMDB endpoint：明确季集只查 TV，其它文件只查 movie。自动候选要求标准化名称与本地化标题、原始标题或 alternative title 的主标题严格相等；别名中的 `$` 只有位于两个 ASCII 英文字母之间时才按风格化 `s` 处理，普通标题不会全局忽略空白。只有本地标题以数字结尾时，才允许远端在同一主标题后用明确分隔符追加副标题。电影发行年和剧集首播年必须完全相同且不去掉年份重试；仅有后续季年份时通过 TV search `year` 和对应 season details 严格验证；没有任何年份时在结果不超过 20 页时遍历全部页并选择完整日期最新作品，最新日期并列时保持未匹配。选中的 provider ID 直接获取详情，不再做类型 detect 或第二次标题搜索。成功后标记 `metadata_status = matched`；无严格候选写入 `no_remote_match`，provider 请求错误写入 `metadata_provider_error`
- 本地分析完成、pending 提交和 remote 完成分别通过 `scan_job_groups` 幂等推进任务计数；同一扫描组的本地 pending 写入和远端最终写入各自使用一个短事务，组内任一文件失败时整组回滚，每个事务只执行一次孤儿结构清理和一次 catalog revision bump
- 每个扫描组进入远端终态并成功提交后，由 remote worker 在任务执行上下文中累计文件状态和非阻断 `ffprobe` 警告；终态事务把统计与最多 20 个问题摘要直接写入通用通知 payload，不维护第二套扫描报告存储
- 剧集身份字段优先读取最近的 `tvshow.nfo`，否则从文件名里的 `SxxExx` 拆出剧名。S01 文件中的明确年份作为系列首播年；S02 及以后文件中的年份只记录为对应季播出年，不能写入系列年份
- 如果文件位于明确的季目录树下，会把共同剧集容器路径当作不透明分组边界，将同一容器内的多季资源合成一个扫描组；目录文字本身不参与标题、别名或年份候选
- 组内存在 S01 时不使用后续季年份。只有缺少 S01、`tvshow.nfo` 也没有系列年份时，才把最早已导入季的文件年份作为 `season_number + season air year` 提示传给 TMDB；搜索使用 TV `year` 参数，候选还必须通过对应 season details 的日期验证且最终唯一
- TMDB 补全成功前，扫描占位和本地入库条目使用本地分析出的电影或剧集名称；TMDB 补全成功后，展示标题必须使用 TMDB 返回的名称覆盖本地名称，后续本地剧集归组只更新 `source_title` / 季集结构，不要让目录名或本地解析名压住远端结果
- TMDB 未启用时完成状态写入 `metadata_status = skipped`；本地分析期间仍按猜测类型展示，完成后因为没有远端类型确认而进入 Other
- 任务进度衡量“本轮处理是否完成”，不衡量 TMDB 匹配成功率；`unmatched`、`failed` 和 `skipped` 条目完成最终状态写入后同样计入任务完成度，避免任务永远停在 99
- 每个扫描展示组会先以本地分析结果 upsert 一次；完成 metadata / 海报后再次调用 `mova-db` 以组级事务覆盖该组文件，并发出带 `poster_path` / `overview` / `metadata_status` / `remote_media_type` 的 `ScanJobEvent::ItemUpdated`。只有严格匹配并绑定 provider ID 的条目才写入 `remote_media_type`；未匹配、失败或跳过的条目进入 Other
- 剧集没有远端集剧照时不再生成视频首帧图写入海报字段；缺图保持为空，避免本地首帧覆盖后续 TMDB 海报语义
- 最后只对缺失路径做删除 reconcile；未变化路径完全保留，不参与重探测和 upsert
- 媒体库的 `metadata_language` 变化时，应用层会把该库全部 `media_items` 标记为 `pending`；随后自动触发的扫描会覆盖全部本地文件并按新语言重新请求远端元数据，同时继续复用指纹未变化文件的本地分析缓存，避免无意义地重复执行 sidecar / `ffprobe`
- 媒体库不再维护启用/禁用状态；创建后始终可扫描，配置更新只包含名称、描述和元数据语言

同名同年的严格候选有多个时，先缩小到 `original_title / original_name` 也与本地主标题严格对齐的候选子集；子集为空时保留原候选，子集仍不唯一时不自动猜测国家。

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
- 未完成播放会按电影或 Series upsert `continue_watching`，同系列切集只更新一个活跃条目
- `continue_watching` 每个用户最多保留 20 条；已完成内容保留进度和完成标记，但从活跃队列删除

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
