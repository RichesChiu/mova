# TMDB 接入契约

本文档定义 Mova 服务端如何使用 TMDB v3 完成作品身份确认、元数据补全、演员按需加载和图片缓存。TMDB 的完整 v3 endpoint 目录见 [`TMDB.md`](TMDB.md)，扫描编排、分组和任务进度见 [`MEDIA_LIBRARY_SCAN.md`](MEDIA_LIBRARY_SCAN.md)。

Web、macOS 和 iOS 客户端不直接访问 TMDB，只消费 Mova HTTP API 和 SSE 同步协议。

## 1. 启用与边界

Mova 使用 TMDB 账户 API 设置页提供的 **API Read Access Token**：

```http
Authorization: Bearer <MOVA_TMDB_ACCESS_TOKEN>
Accept: application/json
```

Token 缺失或只含空白时，服务端使用 disabled provider：

- HTTP 服务、后台 worker 和本地扫描仍可启动。
- 名称解析、NFO/sidecar、`ffprobe`、本地入库和播放保持可用。
- 不发起 TMDB 搜索、详情、演员或图片请求。
- 条目以 `skipped / metadata_provider_disabled` 完成本轮处理。
- 后续配置 Token 并重启后，重新扫描会补做此前跳过且没有 provider binding 的条目。

本地与远端的职责边界：

| 权威来源 | 字段和结构 |
| --- | --- |
| 本地 | 媒体库归属、文件路径和指纹、物理版本、容器与音视频技术信息、字幕、明确的季集坐标、`source_title` |
| TMDB | 已接受身份的规范标题、原始标题、正式年份、简介、国家/地区、题材、制作方、评分、外部 ID 和远端图片 |
| 用户/NFO | 用户手动选择的 provider ID 是明确身份；NFO 和 sidecar 提供自动扫描前的已有字段 |

TMDB 命中不会把电影改成剧集或重建本地季集坐标。结构由本地证据决定，TMDB 只在对应类型内确认作品身份并提供规范元数据。

## 2. 当前已实现的 endpoint

实现位于 `crates/mova-application/src/metadata.rs`。默认 API base URL 为 `https://api.themoviedb.org/3`，默认图片 base URL 为 `https://image.tmdb.org/t/p/original`；连接超时 4 秒、单次请求超时 12 秒。媒体库语言当前支持 `zh-CN` 和 `en-US`。

扫描任务在内存中维护有界请求缓存。已有 provider ID 的请求以“媒体类型 + 语言 + provider ID”为唯一键，不受不同本地文件标题、年份或季提示影响；搜索请求则使用规范化标题、严格年份、季验证提示、媒体类型和语言。元数据详情与剧集季集大纲分别缓存，明确未命中也可以复用，临时网络或 provider 错误不进入缓存。该缓存只负责一次扫描执行内的请求去重，不替代数据库中的 provider binding，也不跨进程提供可靠状态。

| Endpoint | 当前用途 |
| --- | --- |
| `GET /3/search/movie` | 无完整季集坐标的自动匹配、手动候选搜索 |
| `GET /3/search/tv` | 具有完整季集坐标的自动匹配、手动候选搜索 |
| `GET /3/movie/{id}/alternative_titles` | 直接标题均未严格命中时验证电影别名 |
| `GET /3/tv/{id}/alternative_titles` | 直接标题均未严格命中时验证剧集别名 |
| `GET /3/movie/{id}?append_to_response=external_ids,images` | 电影详情、评分、外部 ID 和图片集合 |
| `GET /3/tv/{id}?append_to_response=external_ids,images` | 剧集详情、评分、外部 ID、图片集合和季摘要 |
| `GET /3/tv/{id}/season/{season_number}` | 后续季年份验证及本地季集大纲 |
| `GET /3/movie/{id}/credits` | 电影演员首次按需加载 |
| `GET /3/tv/{id}/aggregate_credits` | 剧集演员首次按需加载 |

详情请求通过 `append_to_response=external_ids,images` 合并外部身份和图片集合。当前实现没有调用 `/configuration`，图片 URL 仍使用运行时配置或默认图片 base URL；也没有 append `release_dates` 或 `content_ratings`。

演员不在全库扫描阶段预取。客户端调用 `GET /api/media-items/{id}/cast` 时，服务端先读本地缓存；缺失时按已绑定 provider ID 获取并持久化全部有效演员。演员失败不阻断媒体详情主体。

## 3. 身份来源与唯一类型

身份来源优先级：

1. 用户手动选择的 TMDB ID。
2. 已经成功绑定且类型一致的 TMDB ID。
3. 最近 `tvshow.nfo` 中的系列标题和年份。
4. 文件名分析出的标题和年份。

自动查询类型只由完整季集坐标决定：

```text
season_number != null AND episode_number != null
    -> GET /3/search/tv

otherwise
    -> GET /3/search/movie
```

- 不带完整季集坐标的文件按电影查询。
- 自动扫描不会同时搜索 movie 和 TV。
- 对应 endpoint 没有严格候选时保持未匹配，不跨类型兜底。
- 搜索选中的 provider ID 直接进入详情请求，不再执行第二轮标题搜索。
- 手动匹配仍限定在当前本地结构对应的类型；改变本地结构需要独立的人工重分类能力。

## 4. 标题标准化与候选阶段

标题标准化只消除排版差异：

- Unicode 小写化。
- 删除首尾空白并压缩连续空白。
- 统一点号、下划线、连字符、全角/半角空格和常见引号。
- 忽略 `·`、`・`、`•` 等装饰性间隔号。
- `$` 只有位于两个 ASCII 英文字母之间时才按风格化字母 `s` 处理。

不使用普通前缀、包含、编辑距离、分词相似度、popularity 或评分模型。

候选按以下顺序分阶段收口，首个非空阶段会丢弃所有较弱阶段：

1. 完整原始标题。
2. 完整本地化标题。
3. 数字结尾主标题的原始标题副标题兼容。
4. 数字结尾主标题的本地化标题副标题兼容。

副标题兼容只在完整标题阶段没有候选时启用。本地主标题必须以 ASCII 数字结尾，远端只能在完全相同的主标题后用 `:`、`：`、`|`、`｜`、`–` 或 `—` 追加非空副标题。

只有直接标题阶段完全没有候选时，才调用 alternative titles。别名验证仍按完整相等优先、数字副标题兼容次之，不产生分数。最多验证 40 个候选，避免无界 N+1 请求。

## 5. 年份规则

### 5.1 电影和剧集首播年

- movie 对齐 `release_date` 年份，并在搜索时传 `primary_release_year`。
- TV 系列年份只来自 `tvshow.nfo` 或 S01 文件名，对齐 `first_air_date`，并传 `first_air_date_year`。
- 名称和年份必须同时满足；相差 1 年也不接受。
- 带年份搜索没有结果时，不移除年份重试。
- 本地有年份而候选缺少正式日期时，不能自动接受。
- 同一标题阶段仍有多个身份时保持未匹配。

### 5.2 后续季年份

- S02 及以后文件名中的年份只表示对应季播出年，不写入 series `year`。
- 同组存在 S01 时，后续季年份不参与查询。
- 只有缺少 S01、`tvshow.nfo` 系列年份也为空时，才使用最早已导入季的 `season_number + season air year`。
- TV search 传 `year` 后，再读取候选的对应 season details。
- season 或其中 episode 的播出年必须匹配，验证后候选必须唯一。
- 绑定成功后，series `year` 始终取 TV details 的 `first_air_date`。

### 5.3 无年份

- 搜索不传年份。
- 当结果不超过 20 页时遍历全部页，再执行严格标题过滤。
- movie 按完整 `release_date`、TV 按完整 `first_air_date` 降序。
- 只接受完整日期唯一最新的身份。
- 最新日期并列，或所有严格候选都缺少日期时，保持未匹配。

## 6. 匹配结果与字段覆盖

匹配结果：

| 状态 | 原因 | 含义 |
| --- | --- | --- |
| `matched` | `null` | 接受唯一 TMDB 身份并完成规范字段写入 |
| `unmatched` | `no_remote_match` | 指定类型中没有唯一严格候选 |
| `failed` | `metadata_provider_error` | TMDB 请求、超时或响应处理失败 |
| `skipped` | `metadata_provider_disabled` | 没有启用 TMDB provider |

自动扫描接受身份后：

- 保留媒体库、物理文件、版本关系、季集坐标和 `source_title`。
- 非空远端展示标题会替换 `title`。
- `original_title`、年份、国家、题材、制作方、简介、海报和背景只在现有字段为空时补入；NFO/sidecar 已有的这些字段会保留。
- 外部 ID 和 TMDB 评分使用本次远端响应替换。
- 远端 Logo 存在，或当前 Logo 为空/仍是远程 URL 时，更新 `logo_path`。
- movie/series poster、backdrop、Logo、season poster 和 episode still 按自身层级写入，不互相兜底。
- 具有相同 TMDB movie ID 的本地文件归并为同一电影的多个播放版本。
- 具有相同 TMDB series ID 的本地剧集组归并为同一 series；同一季集坐标的物理文件成为多个播放版本。

用户手动选择候选时，选中的 provider ID 是明确身份，远端标题、身份、评分、外部 ID 和图片会按替换流程写回；远端没有 poster、backdrop 或 Logo 时，对应远端图片字段可以清空。扫描自动补全与人工替换是两种不同的覆盖强度，客户端不得把自动匹配理解为无条件覆盖所有 NFO 字段。

`remote_media_type` 只在绑定远端身份时写入。客户端不得根据语言、国家或搜索顺序伪造远端类型。

## 7. 当前字段映射

| Mova 语义 | TMDB 来源 |
| --- | --- |
| provider identity | `id` + movie/TV endpoint 类型 |
| 展示标题 | movie `title` / TV `name` |
| 原始标题 | movie `original_title` / TV `original_name` |
| 年份 | movie `release_date` / TV `first_air_date` |
| 简介 | `overview` |
| 国家/地区 | movie `production_countries` / TV `origin_country` |
| 题材 | `genres` |
| 制作方 | `production_companies` |
| 评分 | `vote_average`、`vote_count` |
| 图片 | 默认 poster/backdrop 与 `images.logos` 中选中的 Logo |
| 外部身份 | IMDb、Wikidata、Facebook、Instagram、Twitter；TV 额外包含 TVDB |

评分保存到 `media_item_ratings`：

- 当前写入 `source=tmdb`、`kind=audience`、`scale=10`。
- `vote_count = 0` 或评分无效时不创建记录。
- 外部 ID 只承担跨来源身份，不代表已接入对应平台评分。
- 未来增加 IMDb、Rotten Tomatoes 等来源时，不改变 TMDB 评分语义。

provider ID 和外部 ID 以非空字符串存储和传输，客户端不得依赖 TMDB 数字 ID 的范围。

## 8. 图片与 Logo

movie 和 TV details 通过 append images 返回 `posters`、`backdrops` 和 `logos`。当前 schema 保存选中的 poster、backdrop 和 Logo，不保存完整候选集合。

Logo 语言顺序：

- 非中文库：媒体库语言、英文、无语言素材。
- `zh-CN`：英文、无语言素材、TMDB 标记为 `zh` 的素材。

同语言候选按投票均值、投票数和像素面积选择。选中的图片下载到媒体库独立缓存目录，再通过 Mova 稳定 URL 提供给客户端。没有合适素材时对应字段保持为空。

远端图片下载只接受 `https://image.tmdb.org/t/p/` 下的地址，或者管理员通过 `MOVA_TMDB_IMAGE_BASE_URL` 显式配置的来源；显式来源按解析后的 scheme、host、有效端口和路径边界校验，候选 URL 不得携带 query 或 fragment，重定向目标也必须满足同一约束。连接超时为 4 秒，完整请求超时为 15 秒，单张图片最多 20 MiB；响应必须同时通过受支持的图片 MIME 类型与文件头校验。下载先写入缓存目录内的临时文件，完整写入并同步后通过同文件系统原子重命名发布，客户端不会读取到半成品。

API 响应只直接透出不带 query/fragment 的 TMDB 官方 HTTPS 图片地址；显式配置的服务端图片代理只用于受控下载，不作为客户端直连地址。已缓存图片和 NFO sidecar 图片通过 Mova 内部路由读取，并在响应前再次校验所属媒体库边界、20 MiB 上限和图片文件头。

## 9. 请求与失败策略

- 单个补全上下文会按完整 `MetadataLookup` 缓存查询结果；key 包含类型、语言、标题、作品年份、可选季年份提示和 provider ID。
- 同一扫描任务内的后续扫描组可以复用已经完成的查询结果；缓存不跨进程，也不承担持久化职责。
- `401/403` 表示服务配置错误。
- `404` 表示已有 binding 可能失效，需要复核。
- `429/5xx/timeout` 是可重试 provider 故障，不得写成 `no_remote_match`。
- provider 请求在补全过程中失败时，扫描组恢复远端处理前的本地权威快照；不得清空既有 provider binding、标题、简介、图片、评分、external IDs 或 NFO 字段。此前已经持有 binding 的文件保持 `matched`，同时以 `metadata_provider_error` 记录本次刷新故障并在后续扫描重试；同组新版本或新单集可继承该已接受的作品身份以定位共享电影或剧集条目，但其自身仍标记为 `failed / metadata_provider_error`，不得伪装为已经完成远端补全，也不能在组事务中反向覆盖共享父条目。
- TMDB 配置暂时不可用时，已有 binding 的条目保持已匹配数据；只有从未绑定的新条目标记为 provider disabled。
- 评分、external IDs 和 artwork 只有在本次成功取得并应用 TMDB 详情时才允许替换或清空。查询未命中、已有完整元数据而跳过查询、provider disabled 和 provider 临时失败都属于非权威提交。非权威提交可以把既有受信任远端图片 URL 替换为本轮已经校验并原子发布的非空本地缓存路径，但不得清空图片或替换成另一条远端 URL。
- 评分或图片处理失败不得把已经接受的身份伪装成严格匹配失败。
- 网络、图片下载和文件 I/O 必须在数据库事务外完成。

## 10. 规划能力

以下能力尚未进入当前运行时契约：

- 调用 `/configuration` 动态读取图片 base URL 和尺寸。
- append `release_dates` 与 `content_ratings`。
- 保存完整图片候选集合并允许切换。
- 关键词、视频、推荐、相似内容和观看渠道。
- 人物详情、合集和非标准 episode groups。
- 统一的 provider 全局 rate limiter、`Retry-After` 和带抖动退避。
- 跨 worker 的查询缓存和 single-flight，避免不同扫描任务同时请求相同 lookup。

规划能力不得在 [`API.md`](API.md) 中描述成已经可用的客户端功能；落地时需要同步代码、schema、API 和本文。
