# TMDB 接入契约

本文档定义 Mova 服务端如何使用 TMDB v3 完成作品身份确认、元数据补全、演员按需加载和图片缓存。TMDB 的完整 v3 endpoint 目录见 [`TMDB.md`](TMDB.md)，扫描编排、分组和任务进度见 [`MEDIA_LIBRARY_SCAN.md`](MEDIA_LIBRARY_SCAN.md)，本地 NFO 的字段与来源契约见 [`NFO_METADATA.md`](NFO_METADATA.md)。

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
| 本地 | 媒体库归属、文件路径和指纹、物理版本、受限业务容器、容器与音视频技术信息、字幕、明确的季集坐标、`source_title` |
| 人工操作 | 管理员明确接受的 provider binding 与人工保存字段；自动扫描不得静默换绑 |
| NFO | 选中、层级正确的本地文档所声明的展示字段、external IDs、评分、演职员和图片；未声明字段可以由 TMDB 补齐 |
| TMDB | 已接受身份的规范标题、原始标题、正式年份、简介、国家/地区、题材、制作方、评分、外部 ID 和远端图片 |

TMDB 命中不会把电影改成剧集或重建本地季集坐标。结构由本地证据决定，TMDB 只在对应类型内确认作品身份并提供规范元数据。

### 1.1 归属与品牌

所有展示 TMDB 数据或图片的 Mova 界面都必须遵守 TMDB 官方归属要求：

- 使用 TMDB 官方批准的 Logo，并让其视觉显著性低于 Mova 主品牌。
- 不修改 Logo 的颜色或纵横比，不翻转或旋转 Logo。
- 在 About、Credits 或同类归属入口显著保留以下英文原文；中文界面说明不能替代该原文：

> This product uses the TMDB API but is not endorsed or certified by TMDB.

Web 客户端通过账户菜单中的 `/about` 提供 About & Credits；官网通过页脚中的 `/credits` 提供 Credits & Data Sources。两处 Logo 都链接到 `https://www.themoviedb.org`。

项目内使用 TMDB 官方 [Logos & Attribution](https://www.themoviedb.org/about/logos-attribution) 页面提供的 **Alt short (blue) - SVG**，原始资产 URL 为：

```text
https://www.themoviedb.org/assets/2/v4/logos/v2/blue_short-8e7b30f73a4020692ccca9c88bafe5dcb6f8a62a4c6bc55cd9ba82bb2cd95f6c.svg
```

该文件原始 SHA-256 为 `8e7b30f73a4020692ccca9c88bafe5dcb6f8a62a4c6bc55cd9ba82bb2cd95f6c`，本地副本不得重新导出或改写。具体归属原文与品牌规则以 TMDB 官方 [FAQ](https://developer.themoviedb.org/docs/faq) 为准。

## 2. 当前已实现的 endpoint

实现位于 `crates/mova-application/src/metadata.rs`。默认 API base URL 为 `https://api.themoviedb.org/3`，默认图片 base URL 为 `https://image.tmdb.org/t/p/original`；连接超时 4 秒、单次请求超时 12 秒。媒体库语言当前支持 `zh-CN` 和 `en-US`。

扫描任务在内存中维护有界请求缓存。已有 provider ID 的请求以“媒体类型 + 语言 + provider ID”为唯一键，不受不同本地文件标题、年份或季提示影响；搜索请求则使用规范化标题、严格年份、季验证提示、媒体类型和语言。元数据详情与剧集季集大纲分别缓存，明确未命中也可以复用，临时网络或 provider 错误不进入缓存。该缓存只负责一次扫描执行内的请求去重，不替代数据库中的 provider binding，也不跨进程提供可靠状态。

| Endpoint | 当前用途 |
| --- | --- |
| `GET /3/search/movie` | 无完整季集坐标且没有直接 ID 提示的自动匹配、手动候选搜索 |
| `GET /3/search/tv` | 具有完整季集坐标且没有直接 ID 提示的自动匹配、手动候选搜索 |
| `GET /3/movie/{id}/alternative_titles` | 直接标题均未严格命中时验证电影别名 |
| `GET /3/tv/{id}/alternative_titles` | 直接标题均未严格命中时验证剧集别名 |
| `GET /3/movie/{id}?append_to_response=external_ids,images` | 电影详情、评分、外部 ID 和图片集合 |
| `GET /3/tv/{id}?append_to_response=external_ids,images` | 剧集详情、评分、外部 ID、图片集合和季摘要 |
| `GET /3/tv/{id}/season/{season_number}` | 后续季年份验证及本地季集大纲 |
| `GET /3/movie/{id}/credits` | 电影演员首次按需加载 |
| `GET /3/tv/{id}/aggregate_credits` | 剧集演员首次按需加载 |

详情请求通过 `append_to_response=external_ids,images` 合并外部身份和图片集合。当前实现没有调用 `/configuration`，图片 URL 仍使用运行时配置或默认图片 base URL；也没有 append `release_dates` 或 `content_ratings`。

演员不在全库扫描阶段为没有本地演职员的条目预取。选中 NFO 包含演员时，服务端持久化并优先返回完整 NFO 演员集合；普通浏览只有在客户端调用 `GET /api/media-items/{id}/cast` 且缓存缺失时，才按已绑定 provider ID 获取并持久化全部有效 TMDB 演员。管理员显式执行元数据匹配或单条元数据刷新时，也会在接受 binding 后立即同步该条目的 TMDB 演员。TMDB 演员请求失败不阻断媒体详情主体；一旦存在远端演员缓存，后续 TMDB 合规复核会与作品详情一起更新它，但不会在全库扫描中为其它条目预取演员。

## 3. 身份来源与唯一类型

人工选择的 TMDB ID 持久化为当前媒体项的明确 binding。自动扫描按以下规则确定远端查询输入：

1. 当前文件或扫描组已经接受且类型一致的 binding 按 ID 刷新；NFO 不能静默替换该身份。
2. 未绑定条目收集受限业务容器名称末尾的显式 TMDB ID、层级正确的 NFO TMDB ID，以及本轮文件树中同业务容器唯一且类型一致的既有 binding。
3. direct lookup 提示去重后只有一个值时按对应 movie 或 TV details endpoint 验证；详情成功返回并应用后才持久化为 binding。
4. 不同来源给出多个 ID 时记录身份冲突，不调用 TMDB，也不按来源顺序盲选。
5. 没有 direct lookup ID 时，使用本地标题与年份执行搜索。

同容器 binding 索引包含本轮仍然存在但因增量扫描而完全复用的文件，不包含数据库中已经从当前文件树消失的旧路径。剧集可以使用该索引；电影只有在文件名没有可用作品标题并采用直接父目录作为业务容器时使用该索引。

NFO ID 按根元素隔离：movie lookup 只读取 `movie` 根，series lookup 只读取 `tvshow` 根。`episodedetails` 中的 TMDB ID 只作为 episode external ID 持久化，不能成为 series direct lookup 提示，也不能绑定或替换父剧。完整选择与冲突规则见 [`NFO_METADATA.md`](NFO_METADATA.md)。

标题来源与 ID 来源分别处理。剧集标题和年份按以下优先级确定：

1. 当前媒体库根目录内最近且被稳定选中的 `tvshow.nfo` 中的非空系列标题和年份。
2. 文件名中完整季集标记之前的明确系列标题；S01 文件年份可以表示系列首播年。
3. 文件名没有系列标题时，受限业务容器的标题和年份。

电影标题搜索输入来自选中 `movie` NFO 的非空标题；NFO 未声明标题时，有明确文件标题则使用文件标题，文件名只包含年份、技术规格或发布标签时使用直接父目录的标题和年份。电影 NFO 的本地展示字段按字段所有权规则保留。文件名没有标题的容器回退、根目录边界和无效候选停止规则见 [`MEDIA_LIBRARY_SCAN.md`](MEDIA_LIBRARY_SCAN.md)。

容器名称末尾支持由 ASCII 十进制数字组成的正整数显式 ID：

```text
作品名 (2026) {tmdb-123456}
作品名 (2026) [tmdbid-123456]
```

`tmdb-` 与 `tmdbid-` 前缀大小写不敏感，并且只在名称末尾的 `{}` 或 `[]` 内识别；标记不参与标题和年份查询。该值在本地分析阶段只是 direct lookup hint，不是已经接受的 binding。服务端先按本地结构选择 movie 或 TV details endpoint，只有详情成功返回并应用后才把响应中的 provider ID 持久化。provider 未启用、请求失败或详情未命中时，不把提示伪装为成功 binding。

自动查询类型只由完整季集坐标决定：

```text
season_number != null AND episode_number != null
    -> GET /3/search/tv

otherwise
    -> GET /3/search/movie
```

- 不带完整季集坐标的文件按电影查询。
- `S01E01.mkv`、`1x02.mkv` 等只有完整季集坐标而没有系列标题的文件仍按 TV 查询；系列标题从 `tvshow.nfo` 或受限容器取得。
- 自动扫描不会同时搜索 movie 和 TV。
- 对应 endpoint 没有符合当前年份策略的候选时保持未匹配，不跨类型兜底。
- 搜索选中的 provider ID 直接进入详情请求，不再执行第二轮标题搜索。
- direct lookup ID 只进入对应类型的 details 请求，不调用 search；详情未命中时也不移除 ID 改做标题搜索。
- 手动匹配仍限定在当前本地结构对应的类型；改变本地结构需要独立的人工重分类能力。

## 4. 标题标准化与候选阶段

本节的标题候选规则只适用于没有 direct lookup hint 的请求。显式容器 ID、层级正确且唯一的 NFO TMDB ID 或同容器唯一既有 binding 生成的提示只产生一个 provider ID 请求，不生成标题搜索候选；当前条目已经接受的 binding 仍优先按 ID 刷新，并遵守既有 binding 的失败保留规则。

本地标题选择先遵守第 3 节的 NFO、文件名和受限容器优先级，再执行以下标准化与远端候选收口。受限容器只选择一个业务目录：剧集允许跳过明确的季目录和纯技术目录，但在第一个非结构目录处停止；电影只检查直接父目录。候选无效时不继续向更高层寻找，媒体库根目录本身永远不是标题候选。

标题标准化只消除排版差异：

- Unicode 小写化。
- 删除首尾空白并压缩连续空白。
- 统一点号、下划线、连字符、全角/半角空格和常见引号。
- 忽略 `·`、`・`、`•` 等装饰性间隔号。
- 完整标题没有命中时，允许 `:`、`：`、`|`、`｜`、`/`、`／`、`-`、`–`、`—` 这类明确分隔符及其相邻空白存在差异，但移除分隔符后的全部文字必须完整一致。
- `$` 只有位于两个 ASCII 英文字母之间时才按风格化字母 `s` 处理。

不使用普通前缀、包含、编辑距离、分词相似度、popularity 或评分模型。

本地带作品年份或后续季年份提示时，候选按以下顺序分阶段收口，首个非空阶段会丢弃所有较弱阶段：

1. 完整原始标题。
2. 完整本地化标题。
3. 仅存在明确分隔符差异的原始标题。
4. 仅存在明确分隔符差异的本地化标题。
5. 数字结尾主标题的原始标题副标题兼容。
6. 数字结尾主标题的本地化标题副标题兼容。

分隔符兼容至少要求一侧实际包含明确分隔符，不能把普通文字增删或任意相似标题降级成同一标题。副标题兼容只在完整标题和分隔符兼容阶段都没有候选时启用。本地主标题必须以 ASCII 数字结尾，远端只能在完全相同的主标题后用 `:`、`：`、`|`、`｜`、`–` 或 `—` 追加非空副标题。

只有直接标题阶段完全没有候选时，才调用 alternative titles。别名验证仍按完整相等、明确分隔符差异、数字副标题兼容的顺序收口，不产生分数。最多验证 40 个候选，避免无界 N+1 请求。无年份查询按 5.3 的首屏精准标题优先、provider 顺序兜底规则处理，不调用 alternative titles。

## 5. 年份规则

### 5.1 电影和剧集首播年

- movie 对齐 `release_date` 年份，并在搜索时传 `primary_release_year`。
- TV 系列年份来自 `tvshow.nfo`、S01 文件名或受限系列容器，对齐 `first_air_date`，并传 `first_air_date_year`。
- 文件名没有可用电影标题时，受限电影容器中的年份作为 movie 年份。
- 名称和年份必须同时满足；相差 1 年也不接受。
- 带年份搜索没有结果时，不移除年份重试。
- 本地有年份而候选缺少正式日期时，不能自动接受。
- 同一标题阶段仍有多个身份时保持未匹配。
- direct lookup 已经由 provider ID 确定身份，不使用本地标题或年份重新筛选另一个候选；详情中的正式年份仍按字段映射写入。

### 5.2 后续季年份

- S02 及以后文件名中的年份只表示对应季播出年，不写入 series `year`。
- 同组存在 S01 时，后续季年份不参与查询。
- 只有缺少 S01、`tvshow.nfo` 系列年份也为空时，才使用最早已导入季的 `season_number + season air year`。
- TV search 传 `year` 后，再读取候选的对应 season details。
- season 或其中 episode 的播出年必须匹配，验证后候选必须唯一。
- 绑定成功后，series `year` 始终取 TV details 的 `first_air_date`。

### 5.3 无年份

- 搜索不传年份。
- 只读取第一页；先在 `original_title/original_name` 与本地化 `title/name` 中查找精准标题命中，命中时采用该候选。
- 没有精准标题命中时，接受 TMDB 响应顺序中的首个结果，以兼容请求语言导致返回标题与本地文件名语言不同的情况。
- 不在本地按日期、热度、语言或国家重新排序。
- 不要求响应语言下的标题字段再次等于查询文本。TMDB 可能根据媒体库语言返回与文件名不同语言的规范标题，但查询结果仍保留相关性排序。
- 搜索没有结果时保持未匹配。
- TV 具有后续季年份提示时不属于无年份兜底，仍按 5.2 完成标题与季播出年验证。

## 6. 匹配结果与字段覆盖

匹配结果：

| 状态 | 原因 | 含义 |
| --- | --- | --- |
| `matched` | `null` | 按 direct lookup 或当前年份策略选定 TMDB 身份并完成规范字段写入 |
| `unmatched` | `no_remote_match` | 指定类型中没有符合当前策略的候选、direct lookup 未命中，或容器身份存在冲突 |
| `failed` | `metadata_provider_error` | TMDB 请求、超时或响应处理失败 |
| `skipped` | `metadata_provider_disabled` | 没有启用 TMDB provider |

自动扫描接受身份后：

- 保留媒体库、物理文件、版本关系、季集坐标和 `source_title`。
- 非空远端展示标题只在标题没有被人工值或选中 NFO 拥有时写入。
- `original_title`、年份、国家、题材、制作方、简介、海报和背景只补充人工值和选中 NFO 没有声明的字段。
- external IDs 和评分按来源共存；本次远端响应只替换 `retrieved_via=tmdb` 的记录，不删除 NFO 或人工来源记录。
- 远端 Logo 只在该字段没有被人工值或选中 NFO 拥有时更新；远端确认没有 Logo 也不能清空本地 Logo。
- movie/series poster、backdrop、Logo、season poster 和 episode still 按自身层级写入，不互相兜底。
- 具有相同 TMDB movie ID 的本地文件归并为同一电影的多个播放版本。
- 具有相同 TMDB series ID 的本地剧集组归并为同一 series；同一季集坐标的物理文件成为多个播放版本。

用户手动选择候选时，选中的 provider ID 是明确身份，远端标题、身份、评分、外部 ID 和图片会按替换流程写回；远端没有 poster、backdrop 或 Logo 时，对应远端图片字段可以清空。扫描自动补全与人工替换是两种不同的覆盖强度，客户端不得把自动匹配理解为无条件覆盖所有 NFO 字段。

`remote_media_type` 只在绑定远端身份时写入。客户端不得根据语言、国家或搜索顺序伪造远端类型。

同一扫描组的 NFO、显式容器 ID 或同容器既有 binding 包含多个不同 TMDB ID 时视为冲突。服务端不调用 TMDB、不自动采用任一 ID，并沿用 `unmatched / no_remote_match`；该情况不增加新的 HTTP 字段或公开原因码。已有人工 binding 时继续保留该身份，冲突 NFO 只保留标准化快照和诊断，不进入公共投影。

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
- 容器 direct lookup hint 的候选集合只包含 provider ID 请求。明确未命中后保持未匹配，不再生成标题候选或调用 search。
- 增量扫描从本轮仍存在的文件建立容器 binding 索引；相同 ID 去重后可以复用，不同 ID 冲突时不发起 provider 请求。
- `401/403` 表示服务配置错误。
- `404` 表示已有 binding 可能失效，需要复核。
- `429/5xx/timeout` 是可重试 provider 故障，不得写成 `no_remote_match`。
- provider 请求在补全过程中失败时，扫描组恢复远端处理前的本地权威快照；不得清空既有 provider binding、标题、简介、图片、评分、external IDs 或 NFO 字段。此前已经持有 binding 的文件保持 `matched`，同时以 `metadata_provider_error` 记录本次刷新故障并在后续扫描重试；同组新版本或新单集可继承该已接受的作品身份以定位共享电影或剧集条目，但其自身仍标记为 `failed / metadata_provider_error`，不得伪装为已经完成远端补全，也不能在组事务中反向覆盖共享父条目。
- TMDB 配置暂时不可用时，已有 binding 的条目保持已匹配数据；只有从未绑定的新条目标记为 provider disabled。显式 ID 或同容器复用 ID 仍只是未验证提示，不会让新条目变成 `matched`。
- 评分、external IDs 和 artwork 只有在本次成功取得并应用 TMDB 详情时才允许替换或清空。查询未命中、已有完整元数据而跳过查询、provider disabled 和 provider 临时失败都属于非权威提交。非权威提交可以把既有受信任远端图片 URL 替换为本轮已经校验并原子发布的非空本地缓存路径，但不得清空图片或替换成另一条远端 URL。
- 评分或图片处理失败不得把已经接受的身份伪装成严格匹配失败。
- 网络、图片下载和文件 I/O 必须在数据库事务外完成。

### 9.1 后台复核与 180 天保留边界

成功写入 TMDB binding 后，服务端维护独立的持久化复核状态。首次 binding 以写入时间建立 150/180 天窗口，不会紧接扫描再做一次重复网络抓取；正常扫描、人工匹配和人工刷新会在写入远端字段的同一数据库事务中保存本次实际响应的 ownership 快照，但不会移动该时钟。只有专用后台复核使用当前已接受 ID 取得 movie details，或同时取得 TV details 与全部季详情，并再次通过 binding/generation CAS 后，才续期。正常目标是在首次 binding 或最近一次严格复核后的第 150 天开始静默复核，为 180 天本地保留上限留出失败重试窗口。复核固定使用当前已经接受的 provider ID、媒体类型和媒体库语言调用 movie 或 TV details endpoint；它不调用 search、alternative titles，也不改变 provider ID、电影/剧集类型、物理版本关系或季集坐标。

调度与限流：

- PostgreSQL 全局最多存在一个 `metadata.tmdb.revalidate` 活跃任务，服务启动只回填复核状态，不为全库一次性创建后台任务。
- 普通复核排在扫描和缓存清理之后；同库扫描运行时不并发写 metadata。180 天到期的本地清理优先于尚未开始的扫描，运行中的扫描仍需先完成当前有 fence 的写入。
- 单次失败任务立即让出全局执行位，条目状态按 15 分钟、1 小时、6 小时、24 小时退避后重新具备入队资格；重启不会丢失该状态。
- Token 缺失时调度器仍运行本地 retention 检查，但不会创建 150 天网络复核任务，也不会发 TMDB 请求；只有条目达到 180 天保留期限时才入队执行本地清理。
- 正常剧集扫描保持部分成功容错：某一季请求失败时，已经取得的季集字段仍可入库并记录实际 ownership 快照，但不会续期整个剧集；条目保持立即可复核，由后续 direct-ID 完整复核补齐。
- 扫描、人工写入和复核都以媒体项及季集的 `updated_at` 做 compare-and-swap；复核时间统一取 PostgreSQL 时钟。相同 provider ID 的 NFO、扫描或人工修改也会让旧复核结果失效，避免晚到响应覆盖新数据。

字段所有权通过上一份成功复核的 TMDB 快照判断，不通过文件路径或字段是否非空猜测：

- `source_title`、`sort_title`、媒体类型、文件、版本关系和季集坐标始终保持本地权威。
- 首次为旧 binding 建立快照时，已有非空标题、简介、年份、国家、题材和制作方若与 direct-ID 响应不同则视为本地值并保留；与响应完全相同的值会在快照提交后建立 ownership。artwork 只有在已位于当前媒体库 `artwork/tmdb` 命名空间，或与随后提交的响应快照一致时才建立 ownership，路径外 sidecar 不会被接管。
- 后续复核只有在当前字段仍等于上一份 TMDB 快照时才更新或清空它；NFO、sidecar 或其它本地流程写成不同值后继续保留。
- 剧集的已持久化季标题、季简介、季 artwork、单集标题、单集简介、单集 artwork，以及 `series_episode_outline_cache` 中经过数据库校验的 `jsonb` 大纲缓存，使用同一快照和 150/180 天生命周期；季集坐标、源标题、文件和播放状态始终保持本地权威。
- TMDB 详情响应产生的 external IDs 和 `source=tmdb` 评分具有明确远端来源，成功复核时权威替换。评分自身的 `fetched_at` 与复核状态的 `verified_at` 分别记录评分和完整详情的获取时间。
- 已存在的 cast cache 通过同一 provider ID 重新获取并更新 `fetched_at`；没有 cast cache 的条目保持按需加载，不因后台任务预取。
- 新 poster、backdrop 和 Logo 必须先通过既有安全下载与原子发布流程进入本地缓存，图片缓存失败会让整次复核进入退避，不会用失败结果替换当前图片。成功换图后，仅删除不再被任何媒体或季引用、且来自上一份 TMDB 快照的缓存文件。

达到 180 天仍未成功复核时，服务端执行一次有明确通知的本地 retention 清理：

- 有可信快照时，仅把仍等于上一份 TMDB 快照的展示字段清除；远端标题回退到 `source_title`。快照始终为空的 binding 若直到 180 天仍未完成一次 direct-ID 复核，已经无法证明哪些 enrichment 可继续保留，因此标题、简介、年份、国家、题材、制作方、季集展示字段和 artwork 均按未知远端 enrichment 清除，只有 `source_title`、`sort_title`、季集坐标、文件和用户状态等本地权威数据保留。
- 清除 TMDB binding、`remote_media_type`、该 binding 写入的 external IDs、TMDB 评分、cast cache 和 series outline cache；媒体项保留并进入 `pending`，可在 Token 恢复后的扫描或人工操作中重新匹配。
- 清除数据库引用的同一事务会持久化 `metadata.tmdb.artwork.cleanup` 后台任务；该任务带租约并以最长 24 小时的封顶退避持续重试直至成功。引用检查和物理删除持有媒体库级 advisory lock，所有可能写入 TMDB artwork 引用的事务使用同一组锁，只对已无任何媒体、季、outline 或演员引用且规范化后仍位于当前媒体库 `artwork/tmdb` 命名空间内的文件执行物理删除。共享图片和路径边界外文件不会删除，进程退出也不会丢失待清理工作；终态任务会立即擦除路径列表，并由有界历史清理回收。为覆盖图片已原子发布、但进程在数据库引用提交前退出的极窄窗口，每个媒体库还会每天持久化一次孤儿扫描任务；该任务不跟随符号链接，只复查并删除修改时间已超过 180 天且仍无引用的 TMDB 缓存文件。
- 写入 library 范围的 warning 通知，`notification_type=metadata.tmdb.retention_expired`、`reason_code=tmdb_retention_expired`；通知和终态复核任务只保留本地定位信息，不继续保存原 TMDB 条目 ID，旧终态任务会按有界批次清理。这不是日常可见的“六个月倒计时”；正常可用的 Token 会在第 150 天后台续期，不改变用户体验。

这套状态属于 1.0 的 `migrations/0001_init.sql` 基线。数据库初始化、升级与数据维护要求见 [`DEPLOYMENT.md`](DEPLOYMENT.md)。

## 10. 规划能力

以下能力尚未进入当前运行时契约：

- 调用 `/configuration` 动态读取图片 base URL 和尺寸。
- append `release_dates` 与 `content_ratings`。
- 保存完整图片候选集合并允许切换。
- 关键词、视频、推荐、相似内容和观看渠道。
- 人物详情、合集和非标准 episode groups。
- 扫描和人工查询共用的 provider 全局 rate limiter、`Retry-After` 和带抖动退避；合规复核已经使用独立的数据库单任务限流和持久退避。
- 跨 worker 的查询缓存和 single-flight，避免不同扫描任务同时请求相同 lookup。

规划能力不得在 [`API.md`](API.md) 中描述成已经可用的客户端功能；落地时需要同步代码、schema、API 和本文。
