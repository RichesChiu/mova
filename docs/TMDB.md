# TMDB 对接契约

媒体库扫描中的调用顺序、分组、任务状态与进度规则见 [`MEDIA_LIBRARY_SCAN.md`](MEDIA_LIBRARY_SCAN.md)。

本文只讨论 Mova 服务端与 TMDB v3 API 的集成。Web、macOS 和 iOS 不应直接访问 TMDB，也不需要知道 TMDB endpoint；三端只消费 Mova 自己的 HTTP API 和 Realtime/SSE 协议。

## 0. 启用方式

Mova 使用 TMDB 账户 API 设置页提供的 **API Read Access Token** 作为 Bearer Token。部署者需要注册并验证 [TMDB](https://www.themoviedb.org/) 账户，进入 [API 设置](https://www.themoviedb.org/settings/api) 申请访问权限，然后把 API Read Access Token 写入运行时环境变量 `MOVA_TMDB_ACCESS_TOKEN`。不要把较短的 `API Key (v3 auth)` 填入该变量，也不要把 Token 提交到仓库。官方认证契约见 [Application Authentication](https://developer.themoviedb.org/v4/docs/authentication-application)。

Token 缺失或只含空白时，服务端构造 disabled provider，不阻止 HTTP 服务、后台 worker 或媒体库扫描启动。扫描保留本地名称解析、NFO/sidecar、`ffprobe`、入库和播放，远端 lookup、详情、outline、评分和 TMDB 图片下载全部跳过，条目终态为 `skipped / metadata_provider_disabled`。配置 Token 并重启后，下一次扫描会重试此前跳过且没有 provider binding 的条目。

## 1. 核心结论

1. TMDB 命中不会等同于“推翻本地结构”。本地文件路径、物理文件版本、剧集季号和集号仍由文件名、NFO 和本地探测决定。
2. 当标题、年份和类型形成可信的同一作品身份后，应绑定 TMDB ID，并采用 TMDB 的规范标题、原始标题、发行信息、简介、题材、国家、制作方、演员和图片等远端元数据。
3. “结构权威”和“元数据权威”是两件事：本地负责回答文件怎样播放和怎样组成季集，TMDB 负责回答这个作品是谁以及怎样展示。
4. 本地名称解析结果直接决定 TMDB endpoint：明确带季集号只查 TV；不带季集号只查 movie。自动扫描不同时搜索两种类型，也不做远端类型评分。
5. 自动匹配不计算分数：主标题必须严格对齐；只有本地标题以数字结尾时，才允许 TMDB 在相同主标题后用明确分隔符追加副标题。电影发行年和剧集首播年必须与 TMDB 正式年份相同；后续季年份只能验证对应季。没有任何年份时，从严格主标题候选中选择正式日期最新者，最新日期并列时保持未匹配。
6. alternative titles 只用于严格别名验证；详情请求附带 `images`，从 Logo 集合选择一张作品标题图并缓存。完整图片集合、分级和上映信息仍按需接入。
7. 一次自动搜索选中的 provider ID 直接进入详情请求，不重复执行类型检测或标题搜索。
8. 演员不在扫库阶段为全部条目预抓。目标是读取单个 Mova 条目详情时由服务端按需获取并持久化演员，并把演员直接放进条目详情响应，客户端不再额外拼接第二个演员请求。

## 2. 本地结构与远端元数据的边界

### 2.1 本地权威字段

以下信息不能被一次 TMDB 搜索结果直接改写：

- 媒体库和根路径归属。
- 物理文件路径、大小、修改时间和扫描指纹。
- 一个电影包含哪些本地 1080p、2160p 或其他版本。
- 文件容器、时长、视频、音频、字幕和 HDR 等技术信息。
- 文件名中明确存在的 `season_number / episode_number`。
- 最近 `tvshow.nfo` 中的系列标题和年份、用户手动选择的 provider ID 和人工覆盖字段。

### 2.2 TMDB 权威字段

自动匹配被接受，或用户手动选择 TMDB 条目后，TMDB 应成为以下规范元数据的来源：

- `tmdb_id` 和远端媒体类型。
- 当前媒体库语言下的展示标题、原始标题。
- 正式发行/首播日期和年份。
- 简介、tagline、原始语言、国家/地区。
- 题材、制作公司；剧集还包括 networks。
- 海报、背景图、作品标题 Logo、季海报和单集剧照。
- 演员、角色和必要的主创信息。
- TMDB 评分统计和外部 ID。IMDb、TVDB、Wikidata 与社交平台 ID 只作为跨来源身份保存，当前不请求 IMDb 或其他评分服务。

优先级应为：用户手动覆盖或明确本地 NFO > 已接受的 TMDB 身份与字段 > 文件名解析回退值。`source_title` 永远保留本地解析结果，不能被 TMDB 标题覆盖。

### 2.3 类型冲突不等于自动重构

以下情况要分开处理：

| 本地证据 | TMDB 结果 | 处理 |
| --- | --- | --- |
| 文件明确为 `S01E01`，TMDB TV 有同名同年剧集 | 只接受 TV ID，沿用本地季集坐标并使用 TMDB 元数据 |
| 文件明确为 `S01E01`，TMDB TV 没有严格匹配 | `unmatched / no_remote_match`；不再额外搜索 movie |
| 文件没有季集号，TMDB movie 有同名同年电影 | 只接受 movie ID，把同组多个清晰度保留为同一电影的多个文件版本 |
| 文件没有季集号，TMDB movie 没有严格匹配 | `unmatched / no_remote_match`；不再额外搜索 TV |
| 名称相同，但本地年份与远端年份不同 | `unmatched / no_remote_match`，不执行去掉年份的宽松重试 |

因此，“名称、年份、类型都对齐”正是应该使用 TMDB 的场景。类型已经由本地季集结构决定，TMDB 只在对应 endpoint 内确认作品身份并补充规范元数据。

## 3. 服务端使用的 TMDB 接口

实现集中在 `crates/mova-application/src/metadata.rs`。所有请求使用 Bearer token，连接超时 4 秒、整次请求超时 12 秒，默认语言为 `zh-CN`，媒体库可以选择 `zh-CN` 或 `en-US`。

| endpoint | 参数 | 触发场景 | 用途和已读取字段 |
| --- | --- | --- | --- |
| `GET /3/search/movie` | `query`、`include_adult=false`、`page`、`language`；有年份时带 `primary_release_year` | 无完整季集坐标的自动匹配、手动候选搜索；缺少 provider ID 时的演员查询 | 自动匹配在结果不超过 20 页时遍历全部页；读取 `id`、标题、原始标题、完整发行日期、简介、海报和背景图 |
| `GET /3/search/tv` | `query`、`include_adult=false`、`page`、`language`；有系列首播年时带 `first_air_date_year`；仅有后续季年份时带 `year` | 明确季集结构的自动匹配、手动候选搜索；缺少 provider ID 时的 outline/演员查询 | 自动匹配在结果不超过 20 页时遍历全部页；读取 `id`、名称、原始名称、完整首播日期、简介、海报和背景图 |
| `GET /3/movie/{movie_id}/alternative_titles` | movie ID | search 候选的标题和原始标题都不满足严格主标题规则时 | 只用于把别名验证为严格主标题匹配，不计算分数 |
| `GET /3/tv/{series_id}/alternative_titles` | series ID | search 候选的名称和原始名称都不满足严格主标题规则时 | 只用于把别名验证为严格主标题匹配，不计算分数 |
| `GET /3/movie/{movie_id}` | `language`、`append_to_response=external_ids,images`、`include_image_language` | 已选电影 ID 的详情补全、手动匹配、刷新元数据 | 标题、原始标题、发行年份、简介、production countries、genres、production companies、poster/backdrop、Logo 集合、TMDB 评分，以及 IMDb、Wikidata、Facebook、Instagram、Twitter 外部身份 |
| `GET /3/tv/{series_id}` | `language`、`append_to_response=external_ids,images`、`include_image_language` | 已选剧集 ID 的详情补全；outline 当前会再次请求一次 | 名称、原始名称、首播年份、简介、origin country、genres、production companies、poster/backdrop、Logo 集合、TMDB 评分，以及 IMDb、TVDB、Wikidata、Facebook、Instagram、Twitter 外部身份和 season summaries |
| `GET /3/tv/{series_id}/season/{season_number}` | `language` | 后续季年份自动匹配验证、扫描剧集图片、剧集详情页 outline、手动匹配剧集 | 自动匹配验证 season `air_date` 或 episode `air_date`；大纲读取季名称、日期、简介、季海报、集号、集标题、简介和 still |
| `GET /3/movie/{movie_id}/credits` | `language` | 电影演员列表首次按需加载或手动替换元数据后 | 演员 ID、姓名、角色、头像和顺序；持久化响应中的全部有效演员 |
| `GET /3/tv/{series_id}/aggregate_credits` | `language` | 剧集演员列表首次按需加载或手动替换元数据后 | 跨全部季集的演员 ID、姓名、角色、头像和顺序；角色优先取覆盖集数最多者，持久化响应中的全部有效演员 |

电影和剧集详情通过 `append_to_response=images` 在同一个请求中获取 Logo 集合，不额外请求独立 `/images` endpoint。海报、背景和 still 使用详情/季详情中的默认 path；选中的图片 path 会拼接到配置的 `https://image.tmdb.org/t/p/original`，然后下载并缓存。

Logo 语言策略：非中文库优先媒体库语言，其次英文、无语言素材；简体中文库优先英文、无语言素材，最后才使用 TMDB 仅标记为 `zh` 的素材，因为该标记不能可靠区分简体、繁体和地区版本。同语言候选依次按投票均值、投票数和像素面积选择。没有合适素材时 `logo_path` 保持为空。

当前评分不产生额外 HTTP 请求：电影和剧集详情响应中的 `vote_average / vote_count` 会作为 `tmdb / audience` 写入通用评分表。`vote_count` 为零或评分无效时不创建评分记录。

## 4. 自动匹配规则

每个扫描组只调用本地结构对应的一类 search，选中 ID 后直接 fetch details。手动搜索同样根据条目类型调用对应 search endpoint。

### 4.1 标题标准化

标题标准化会：

- 转为小写。
- 保留 Unicode 字母和数字，因此中文、韩文等不会被删除。
- 删除英文直/弯单引号，以及 `·`、`・`、`•` 等标题间隔号。
- 其他标点转为空格并压缩连续空格。

标准化后的本地标题必须与 TMDB 本地化标题、原始标题或 alternative title 完全相等。完整相等按“原始标题、本地化标题”的顺序分阶段选择。只有完全没有完整相等候选时，本地标题以数字结尾的续集才允许远端在相同主标题后用 `:`、`：`、`|`、`｜`、`–` 或 `—` 追加非空副标题；兼容阶段同样先检查原始标题，再检查本地化标题。这是明确的主标题边界验证，不是普通前缀、包含、编辑距离或 popularity 匹配。

### 4.2 类型与年份选择

1. 本地同时存在 `season_number + episode_number` 时只搜索 TV；其它文件只搜索 movie。
2. 电影年份对应 `release_date`，剧集系列年份对应 `first_air_date`；请求必须携带对应年份，候选正式日期年份必须完全相同，且不会移除年份重试。
3. 同类型、同年份候选依次经过“完整原始标题、完整本地化标题、编号原始标题兼容、编号本地化标题兼容”四个阶段；首个非空阶段就是唯一有效候选集，后续较弱阶段不得参与竞争。该候选集仍不唯一时保持未匹配，不根据元数据语言猜测制作国家。
4. 剧集组存在 S01 时不使用后续季年份。缺少 S01 和系列年份、但后续季文件有明确年份时，search 传 `year`，再请求每个严格标题候选的对应 season details；季或其集的播出年必须相同，验证后只能剩一个候选。
5. 没有作品年份或季年份时，自动匹配在搜索结果不超过 20 页时遍历全部页，在严格主标题候选中按完整日期选择最新作品；最新日期并列时保持未匹配。超过上限说明查询过宽，自动匹配按未命中收口，交给手动匹配。
6. 全部缺日期或没有严格主标题候选时不自动选择。
7. 已选 provider ID 直接传入详情 endpoint，不再执行第二轮标题搜索。
8. 先从 localized/original title 中寻找严格候选；只有完全没有直接候选时才请求 alternative titles，且最多验证 40 个候选，避免别名验证形成无界 N+1 请求。季年份验证同样最多处理 40 个候选。

### 4.3 后续优化边界

- “所有严格候选都缺少正式日期”和“没有严格候选”都收敛为 `no_remote_match`。
- `apply_remote_metadata()` 对部分已有非空字段采取保留策略；若要让 TMDB 完全成为规范元数据权威，需要统一字段来源与覆盖规则。
- outline 会请求 TV details，并串行遍历 TMDB 返回的全部正数季；可进一步收敛为只拉本地实际存在的季。
- provider 需要统一 rate limiter 和 `429 / Retry-After` 策略。

## 5. 身份与匹配契约：单类型、严格相等、无评分

### 5.1 身份来源优先级

1. 用户在 Mova 中手动选择的 TMDB ID：直接信任该 ID，并按本地结构确定的 endpoint 获取详情，不再搜索。
2. 已经成功绑定的 TMDB ID：继续按 ID 刷新；语言变化不能触发重新搜索和换 ID。
3. 没有可信 ID 时，剧集标题和系列年份优先读取最近的 `tvshow.nfo`，其次使用文件名分析结果；目录文字不参与候选。
4. 使用上述标题和年份，只调用本地结构对应的一个 search endpoint。

### 5.2 类型路由

```text
season_number != null AND episode_number != null
    -> GET /3/search/tv

otherwise
    -> GET /3/search/movie
```

- 文件名拆解明确携带季号和集号，才视为剧集文件。
- 不带完整季集坐标的文件一律按电影查，即使目录名称看起来像电视剧。
- 自动扫描不会为了“确认类型”同时请求 movie 和 TV。
- 对应 endpoint 无严格匹配就是未匹配，不用另一类型兜底。
- 手动匹配仍只能在当前本地结构对应类型内搜索；如果要改变本地结构，需要独立的人工重分类能力，不能伪装成一次 TMDB 匹配。

### 5.3 名称严格相等

名称只做无语义扩张的标准化：

- Unicode 小写化。
- 删除首尾空白并压缩连续空白。
- 统一点号、下划线、连字符、全角/半角空格和常见引号等纯排版差异；`·`、`・`、`•` 等装饰性间隔号直接忽略。
- `$` 只有位于两个 ASCII 英文字母之间时才按风格化字母 `s` 处理；例如 `Cashero` 与 TMDB 别名 `Ca$hero` 可以严格对齐。金额开头的 `$100` 不会因此匹配 `S100`，普通标题也不会全局忽略空白。
- 不做普通前缀、包含、编辑距离、分词相似度或模糊匹配。

标准化后，本地名称必须与以下任一值的主标题完全相等：

- movie `title` 或 `original_title`。
- TV `name` 或 `original_name`。
- 通过对应 `alternative_titles` endpoint 取得的某个别名。

只有完整原始标题和完整本地化标题都没有候选时，才进入副标题兼容阶段。此时本地主标题必须以 ASCII 数字结尾，远端值只能在完全相同的主标题后用 `:`、`：`、`|`、`｜`、`–` 或 `—` 追加非空副标题。例如本地 `东北恋哥3` 可以兼容远端 `东北恋哥3：冬天里的一把火`；本地 `Dune` 不会因此匹配 `Dune: Part Two`。

直接候选按匹配强度分阶段收口：完整原始标题 > 完整本地化标题 > 编号原始标题兼容 > 编号本地化标题兼容。首个非空阶段会丢弃所有较弱候选。例如本地 `John Wick Chapter 2` 与 `John Wick: Chapter 2` 规范化后属于完整原始标题相等，因此不会与 `John Wick Chapter 2: Wick-vizzed` 的副标题兼容候选竞争；`奇遇 (2025)` 的中国候选原始标题同样是“奇遇”，法国候选只有中文翻译名是“奇遇”，因此优先中国候选。该规则依赖作品身份字段，不把 `zh-CN` UI 语言等同于“中国影片”。

TMDB search 会参考翻译名和别名，但 search response 不会指出具体命中的别名。因此当 title/original title 不相等时，可以读取该候选的 alternative titles 做“严格相等验证”；验证仍然只有 true/false，不产生分数。

### 5.4 有年份：名称和年份必须同时相等

- movie 使用本地年份对齐 `release_date` 的年份，并在搜索时传 `primary_release_year`。
- TV 系列年份只来自 `tvshow.nfo` 或 S01 文件名，对齐 `first_air_date` 的年份，并在搜索时传 `first_air_date_year`。
- 名称相等且年份完全相同才接受。
- 同名同年候选只保留首个非空标题匹配阶段；该阶段仍有多个候选时不自动选择。
- 名称相等但年份相差 1 也不接受。
- 带年份搜索无结果时，不移除年份重试。
- TMDB 候选缺少日期而本地携带年份时不能自动接受，进入未匹配。

### 5.5 后续季年份：只验证对应季

- S02 及以后文件名中的年份表示对应季播出年，不表示系列首播年，不写入 series `year`，也不与 `first_air_date` 比较。
- 同一剧集组存在 S01 时，后续季年份不参与 TMDB 查询；S01 没有年份时按无系列年份规则匹配。
- 只有未导入 S01 且没有 `tvshow.nfo` 系列年份时，才使用最早已导入季的 `season_number + season air year`。
- `GET /3/search/tv` 传 `year=<season air year>`，该参数允许 TMDB 按首播日期或任一 episode air date 筛选。
- 对严格标题候选逐一请求 `GET /3/tv/{id}/season/{season_number}`。season `air_date` 或任一 episode `air_date` 的年份必须与本地季年份相同。
- 季验证后的候选必须唯一；不以系列首播日期、搜索顺序、popularity、语言或国家继续猜测。
- 匹配完成后，持久化的 series `year` 始终取 `/3/tv/{id}` 的 `first_air_date`，季年份只属于 season 验证上下文。

### 5.6 无年份：选择严格主标题候选中的最新作品

- 搜索请求不传年份。
- TMDB search 的默认排序不是发布日期排序，因此服务端必须遍历响应声明的全部结果页，再在本地执行严格主标题过滤和日期排序；不能只看第一页就宣称找到了“最新”作品。
- 同一个规范化查询应使用短时缓存和 singleflight 合并，避免并发扫描组重复翻页。
- 先删除所有不满足严格主标题规则的候选。
- movie 按完整 `release_date` 降序；TV 按完整 `first_air_date` 降序。
- 选择日期最新的候选，不能使用 popularity、vote 或 TMDB 返回顺序替代日期；最新完整日期并列时保持未匹配。
- 有日期的候选优先于缺少日期的候选。
- 如果所有严格主标题候选都没有日期，写为 `unmatched / no_remote_match`，交给手动选择。

### 5.7 决策伪代码

```text
kind = has_explicit_season_and_episode ? tv : movie
candidates = search(kind, local_title, series_year_or_season_year_hint)
eligible_candidates = candidates.filter(series_or_movie_year_is_equal)
direct_candidates = first_non_empty(
    eligible_candidates.filter(original_title_exact),
    eligible_candidates.filter(localized_title_exact),
    eligible_candidates.filter(numbered_original_title_with_explicit_subtitle),
    eligible_candidates.filter(numbered_localized_title_with_explicit_subtitle)
)

if direct_candidates is not empty:
    exact_name_candidates = direct_candidates
else:
    exact_name_candidates = first_non_empty(
        eligible_candidates.filter(verified_exact_alternative_title),
        eligible_candidates.filter(verified_numbered_alternative_subtitle)
    )

if later_season_year_hint exists:
    exact_name_candidates = exact_name_candidates.filter(
        candidate_season_or_episode_air_year_is_equal
    )
    accept only when exactly one identity remains
else if movie_or_series_year exists:
    accept only when exactly one identity remains
else:
    matched = exact_name_candidates.sort(remote_full_date desc)
    accept the newest identity only when its full date is unique

no accepted identity -> unmatched / no_remote_match
```

整个流程没有 `MatchRank`、title score、year score、popularity tie-breaker，也没有 opposite-type fallback。

### 5.8 匹配成功后的写入

一旦接受 TMDB ID：

- 保留 `source_title`、物理文件、版本关系和季集坐标。
- 使用 TMDB 当前语言响应覆盖其负责的规范元数据字段，而不是只补空值。
- 用户手动覆盖或 NFO 明确提供的字段继续保留，并记录字段来源，避免下一次刷新把人工选择冲掉。
- 图片按层级写入：series poster/logo、season poster、episode still、movie poster/logo 和 backdrop 不互相替代。
- 同一 provider ID 的电影文件归并为同一电影的多个本地版本。

## 6. 目标 endpoint 组合

TMDB 官方推荐的基本流程也是“先 search，再使用选中的 ID query details”。详情 endpoint 支持 `append_to_response`，可以把同 namespace 的子请求合并到一次 HTTP 请求中。

### 6.1 扫描主链路必须使用

| endpoint | 目标用途 |
| --- | --- |
| `GET /3/configuration` | 获取有效图片 base URL 和尺寸；服务端缓存配置，不再长期硬编码 `original` 地址 |
| `GET /3/search/movie` | 只用于不带季集坐标的组；有年份时传 `primary_release_year`，无结果不去掉年份重试 |
| `GET /3/search/tv` | 只用于明确带季集坐标的组；系列首播年传 `first_air_date_year`，仅有后续季年份时传 `year`，无结果不移除年份重试 |
| `GET /3/movie/{id}?append_to_response=external_ids,images,release_dates` | 一次获取电影详情、外部 ID、图片集合和地区上映/分级信息 |
| `GET /3/tv/{id}?append_to_response=external_ids,images,content_ratings` | 一次获取剧集详情、外部 ID、图片集合和内容分级 |
| `GET /3/tv/{id}/season/{season_number}` | 验证后续季年份，并获取本地实际存在季的季集信息；不默认遍历远端所有季 |

图片 append 请求同时传 `language=<library language>` 和 `include_image_language`。非中文库使用 `<language base>,en,null`，英文库使用 `en,null`；简体中文库使用 `en,null,zh` 并按英文、无语言、中文的顺序选择 Logo。相同语言候选按投票均值、投票数和像素面积排序。

### 6.2 严格别名验证

- `GET /3/movie/{id}/alternative_titles` 和 `GET /3/tv/{id}/alternative_titles` 只在 search 命中、但返回的本地化标题和原始标题都不满足严格主标题规则时使用。
- 有年份时先按远端年份过滤，再验证别名；年份不等的候选不值得追加请求。
- 无年份时对返回候选验证别名后，仍按正式日期选择最新者。
- 别名验证结果缓存到 provider + kind + ID，避免相同候选重复请求。

### 6.3 条目详情时按需获取演员

- `GET /3/movie/{id}/credits` 和 `GET /3/tv/{id}/aggregate_credits` 不在扫库阶段为全部条目预抓。
- 目标 Mova `GET /api/media-items/{id}` 在读取单个条目详情时先查本地演员缓存；缺失时在该请求内按 provider ID 获取演员、持久化并把 top cast 直接放进详情响应。
- 如果条目规范详情也恰好需要刷新，可以在 movie/TV details 中 append credits/aggregate credits，避免同一次详情请求产生两次 TMDB round-trip；否则只请求独立 credits endpoint。
- 演员请求失败不阻断条目详情，`cast` 返回空数组并记录可重试状态。
- 剧集 outline 的季详情按本地季号增量加载和缓存；不为本地不存在的季浪费请求。

### 6.4 暂不在扫描阶段获取

以下数据有价值，但当前 Mova 产品没有对应展示或业务逻辑，不应为了“字段越多越好”增加每次扫库负担：

- videos、recommendations、similar。
- watch providers。
- reviews、lists、changes。
- keywords；以后实现标签搜索或推荐再按需接入。
- 单集独立 details；季详情已经包含本季集列表，只有需要单集 credits 或外部 ID 时再获取。

“尽可能获取足够的信息”应理解为一次身份确认后获取完整、可持久化、当前或近期会使用的规范元数据，而不是无条件抓取 TMDB 的所有子资源。

## 7. 目标字段映射

### 7.1 电影和剧集共同字段

| Mova 语义 | TMDB 来源 |
| --- | --- |
| provider identity | `id` + endpoint 类型 |
| 展示标题 | movie `title` / TV `name` |
| 原始标题 | movie `original_title` / TV `original_name` |
| 完整日期与年份 | movie `release_date` / TV `first_air_date` |
| 简介与短标语 | `overview`、`tagline` |
| 原始语言 | `original_language` |
| 国家/地区 | movie `production_countries` / TV `origin_country` |
| 题材 | `genres`，保留 TMDB genre ID 和本地化名称 |
| 制作方 | `production_companies`，保留 ID、名称和 logo path |
| 评分 | `vote_average`、`vote_count`，保存为来源明确的 TMDB 观众评分 |
| 状态 | movie/TV `status` |
| 图片 | `images.posters / backdrops / logos` 中选定资源及其原始 path、语言、尺寸和 vote |
| 外部身份 | `external_ids` 中 IMDb、Wikidata、Facebook、Instagram、Twitter；TV 额外保留 TVDB |

评分与主元数据字段解耦存储：

- `media_item_external_ids` 使用 `(media_item_id, provider)` 唯一保存 TMDB、IMDb、TVDB、Wikidata 和社交平台等外部身份。`media_items.metadata_provider + metadata_provider_item_id` 单独表达当前主元数据身份；外部身份表只承担跨源关联，不决定主数据归属。
- `media_item_ratings` 使用 `(media_item_id, source, kind)` 唯一保存来源原始分值、量纲、评价数量、获取渠道和获取时间。
- 当前只写入 `source=tmdb`、`kind=audience`、`scale=10`；`retrieved_via=tmdb`。
- `source` 和 `kind` 不使用数据库枚举。未来增加 IMDb、Rotten Tomatoes critics/audience 等来源时不修改表结构。
- 同一 TMDB 身份刷新时只替换 TMDB 评分；只有主元数据提供方的 ID 发生变化时才清除旧身份关联的全部评分。IMDb、TVDB、Wikidata 或社交账号的补充与变化不视为作品身份切换。
- 第三方用户聚合评分与 Mova 用户自己的个人打分是不同数据域，个人打分不得写入 `media_item_ratings`。

### 7.2 电影专有字段

- `runtime`。
- `belongs_to_collection`，为后续电影合集做准备。
- `release_dates` 中按用户/服务器地区选择的正式上映日期和 certification。
- budget/revenue 当前没有产品用途，可以解析但不必进入第一版 schema。

### 7.3 剧集专有字段

- `last_air_date`、`number_of_seasons`、`number_of_episodes`。
- `episode_run_time`、`status`、`in_production`、TV `type`。
- `networks` 和 `created_by`。
- `content_ratings` 中按地区选择的分级。
- season summary 中的 `season_number`、`air_date`、`episode_count`、`poster_path`。

### 7.4 季和单集

- 季：名称、播出日期、简介、季海报。
- 单集：集号、名称、播出日期、简介、时长、still、vote average/count。
- 只把本地实际存在的季集坐标写入可播放结构；TMDB 多出来的未来集可以作为 outline 展示数据，但不能伪装成本地可播放资源。

### 7.5 图片与 Logo 集合

movie 和 TV 的 `/images` 都会返回 `posters`、`backdrops`、`logos`；每个资源包含：

- `file_path`
- `file_type`（部分 Logo endpoint 提供，例如 SVG/PNG）
- `iso_639_1`
- `width / height / aspect_ratio`
- `vote_average / vote_count`

其它图片集合：

- collection images：`posters / backdrops`
- season images：`posters`
- episode images：`stills`
- person images：`profiles`
- company/network images：`logos`

`media_items.logo_path` 保存当前选中的作品标题 Logo，并与 poster/backdrop 一样缓存后对客户端提供稳定 URL。为后续 Logo、海报切换和多语言图片保留标准化集合：

```text
provider_artworks
    id
    owner_type          # movie / series / season / episode / collection / person / company / network
    owner_id
    provider
    provider_item_id
    artwork_type        # poster / backdrop / logo / still / profile
    file_path
    file_type
    language
    width
    height
    aspect_ratio
    vote_average
    vote_count
    is_selected
    cached_path
```

默认展示图只是集合中 `is_selected = true` 的资源；切换展示图不需要重新刮削整个条目。

当前 schema 保存选中的 poster、backdrop 和 Logo，不保存完整候选集合及其语言、尺寸、投票属性。落地完整集合时需要直接调整 pre-1.0 的 `migrations/0001_init.sql`，并同步 Rust domain/DB/API 类型。旧开发数据库不会平滑获得新字段，需要重建 `data/postgres` 并重新扫描。

## 8. 请求、缓存和失败策略

- 本地结构只生成一个 provider kind；自动搜索只允许调用该 kind 对应的 endpoint。
- 搜索结果完成严格过滤后直接把选中 ID 交给详情获取，严禁再次按标题搜索。
- 搜索缓存 key 至少包含 provider、kind、语言、标准化标题、作品年份、可选季号和季年份；相同 key 使用 single-flight。
- 详情缓存 key 包含 provider、类型、ID、语言；图片列表还要包含 image language 策略版本。
- TMDB 使用进程级全局有界并发和 rate limiter。官方当前说明仍可能在约每秒 40 请求附近限制批量抓取，必须尊重 `429` 和 `Retry-After`，并使用带抖动退避。
- `404` 表示绑定可能失效，应进入待复核；`401/403` 是服务配置错误；`429/5xx/timeout` 是可重试 provider 故障，不能写成“无匹配”。
- 评分写入失败不得把已经成功的 TMDB 身份匹配误报成无匹配；网络和 provider 错误继续按可重试故障处理。
- `/configuration` 使用长 TTL 缓存；已有有效缓存时 TMDB 暂时不可用可以继续构造图片地址，首次启动且没有配置时只暂停远端图片下载，不影响本地扫描。
- 图片下载使用 TMDB configuration 返回的合适尺寸。Mova 自己缓存后对客户端提供稳定 URL，不让客户端依赖 TMDB CDN。

## 9. 实现范围

扫描自动匹配提供以下能力：

- 本地结构单类型路由，不执行双类型 detect。
- 主标题、原始标题、alternative titles、电影发行年和剧集首播年的严格验证。
- 后续季年份与系列首播年分离，并通过对应 season details 验证。
- 数字结尾续集的显式副标题边界。
- 无作品年份时选择唯一最新正式日期。
- 搜索选中 provider ID 后直接读取详情，不重复标题搜索。
- 电影与剧集详情附带 Logo 集合，按媒体库语言策略选择、下载并持久化一张标题 Logo。

扩展字段章节中的 configuration、完整 images 候选集合、release dates 和 content ratings 属于规划能力；接入时必须同步本文、`docs/API.md` 和数据模型说明。

## 10. TMDB 官方文档

- [Search & Query For Details](https://developer.themoviedb.org/docs/search-and-query-for-details)
- [Search Movies](https://developer.themoviedb.org/reference/search-movie)
- [Search TV](https://developer.themoviedb.org/reference/search-tv)
- [Movie Details](https://developer.themoviedb.org/reference/movie-details)
- [TV Series Details](https://developer.themoviedb.org/reference/tv-series-details)
- [TV Season Details](https://developer.themoviedb.org/reference/tv-season-details)
- [Append To Response](https://developer.themoviedb.org/docs/append-to-response)
- [Configuration Details](https://developer.themoviedb.org/reference/configuration-details)
- [Image Basics](https://developer.themoviedb.org/docs/image-basics)
- [Image Languages](https://developer.themoviedb.org/docs/image-languages)
- [Movie Images](https://developer.themoviedb.org/reference/movie-images)
- [TV Series Images](https://developer.themoviedb.org/reference/tv-series-images)
- [TV Season Images](https://developer.themoviedb.org/reference/tv-season-images)
- [TV Episode Images](https://developer.themoviedb.org/reference/tv-episode-images)
- [Movie Credits](https://developer.themoviedb.org/reference/movie-credits)
- [TV Series Aggregate Credits](https://developer.themoviedb.org/reference/tv-series-aggregate-credits)
- [Movie Release Dates](https://developer.themoviedb.org/reference/movie-release-dates)
- [TV Series Content Ratings](https://developer.themoviedb.org/reference/tv-series-content-ratings)
- [Movie Alternative Titles](https://developer.themoviedb.org/reference/movie-alternative-titles)
- [TV Series Alternative Titles](https://developer.themoviedb.org/reference/tv-series-alternative-titles)
- [Rate Limiting](https://developer.themoviedb.org/docs/rate-limiting)

## 11. TMDB v3 完整接口目录与 Mova 使用规划

本目录按 2026-07-16 的 [TMDB 官方 OpenAPI](https://developer.themoviedb.org/openapi/tmdb-api.json) 整理。它记录接口能力和 Mova 采用层级，不表示所有接口都要在扫描阶段调用。

状态含义：

- `当前`：现有 Rust 代码已经调用。
- `核心`：目标扫描或详情链路需要。
- `按需`：用户打开对应详情或功能时调用。
- `预留`：文档保留，后续产品功能需要时接入。
- `不接入`：依赖 TMDB 用户账户、写操作或与 Mova 自托管模型重复。

### 11.1 基础配置、查找、搜索与发现

| Method | Path | 能力 | Mova 规划 |
| --- | --- | --- | --- |
| `GET` | `/3/authentication` | 验证 API key/token | `核心`，服务启动或配置检查时使用 |
| `GET` | `/3/configuration` | 图片 base URL、secure base URL 和各类有效尺寸 | `核心` |
| `GET` | `/3/configuration/countries` | ISO 3166-1 国家列表 | `预留`，地区设置 |
| `GET` | `/3/configuration/jobs` | 演职员部门和 job 列表 | `预留`，主创结构化 |
| `GET` | `/3/configuration/languages` | ISO 639-1 语言列表 | `预留`，扩展元数据语言 |
| `GET` | `/3/configuration/primary_translations` | TMDB 主要翻译语言 | `预留` |
| `GET` | `/3/configuration/timezones` | 国家与时区映射 | `预留` |
| `GET` | `/3/certification/movie/list` | 电影分级体系 | `预留`，分级展示/过滤 |
| `GET` | `/3/certification/tv/list` | TV 分级体系 | `预留` |
| `GET` | `/3/genre/movie/list` | 电影 genre 字典 | `预留`，详情已直接返回 genres |
| `GET` | `/3/genre/tv/list` | TV genre 字典 | `预留` |
| `GET` | `/3/find/{external_id}` | 用 IMDb、TVDB、Wikidata 等外部 ID 反查 TMDB | `预留`，NFO 非 TMDB ID 对接 |
| `GET` | `/3/search/movie` | 按原始、翻译和别名搜索电影 | `当前`，目标只用于无季集坐标文件 |
| `GET` | `/3/search/tv` | 按原始、翻译和别名搜索 TV | `当前`，目标只用于明确季集文件 |
| `GET` | `/3/search/multi` | 同时搜索 movie、TV、person | `不接入`，违反单类型路由规则 |
| `GET` | `/3/search/person` | 搜索人物 | `预留` |
| `GET` | `/3/search/collection` | 搜索电影合集 | `预留` |
| `GET` | `/3/search/company` | 搜索制作公司 | `预留` |
| `GET` | `/3/search/keyword` | 搜索关键词 | `预留` |
| `GET` | `/3/discover/movie` | 多条件发现电影 | `预留`，以后实现远端发现页时使用 |
| `GET` | `/3/discover/tv` | 多条件发现 TV | `预留` |
| `GET` | `/3/trending/all/{time_window}` | 全类型趋势 | `预留` |
| `GET` | `/3/trending/movie/{time_window}` | 电影趋势 | `预留` |
| `GET` | `/3/trending/tv/{time_window}` | TV 趋势 | `预留` |
| `GET` | `/3/trending/person/{time_window}` | 人物趋势 | `预留` |
| `GET` | `/3/watch/providers/regions` | 流媒体 provider 可用地区 | `预留` |
| `GET` | `/3/watch/providers/movie` | 电影 provider 字典 | `预留`，使用时遵守 JustWatch attribution |
| `GET` | `/3/watch/providers/tv` | TV provider 字典 | `预留`，使用时遵守 JustWatch attribution |

### 11.2 Movie

| Method | Path | 能力 | Mova 规划 |
| --- | --- | --- | --- |
| `GET` | `/3/movie/{movie_id}` | 电影完整详情 | `当前`，目标 `核心` |
| `GET` | `/3/movie/{movie_id}/alternative_titles` | 各地区别名 | `核心`，只用于严格名称验证 |
| `GET` | `/3/movie/{movie_id}/credits` | 电影 cast/crew | `当前`，目标在 Mova 条目详情中按需获取 |
| `GET` | `/3/movie/{movie_id}/external_ids` | IMDb、Wikidata、社交 ID | `当前`通过 movie details append 获取并持久化全部支持字段 |
| `GET` | `/3/movie/{movie_id}/images` | `posters / backdrops / logos` 集合 | `核心`，目标通过 append 获取 |
| `GET` | `/3/movie/{movie_id}/release_dates` | 各地区发行日期、类型和 certification | `核心`，目标通过 append 获取 |
| `GET` | `/3/movie/{movie_id}/keywords` | 电影关键词 | `预留`，标签/推荐功能 |
| `GET` | `/3/movie/{movie_id}/translations` | 标题、简介等翻译集合 | `预留`，多语言离线缓存 |
| `GET` | `/3/movie/{movie_id}/videos` | 预告片、花絮等视频 | `预留` |
| `GET` | `/3/movie/{movie_id}/recommendations` | 推荐电影 | `预留` |
| `GET` | `/3/movie/{movie_id}/similar` | 相似电影 | `预留` |
| `GET` | `/3/movie/{movie_id}/reviews` | 用户评论 | `预留` |
| `GET` | `/3/movie/{movie_id}/lists` | 包含该电影的 TMDB 列表 | `预留` |
| `GET` | `/3/movie/{movie_id}/watch/providers` | 分地区观看渠道 | `预留`，使用时标注 JustWatch |
| `GET` | `/3/movie/{movie_id}/changes` | 单片近期变更 | `预留`，增量元数据刷新 |
| `GET` | `/3/movie/{movie_id}/account_states` | TMDB 账户收藏/评分状态 | `不接入` |
| `POST/DELETE` | `/3/movie/{movie_id}/rating` | 写入/删除 TMDB 用户评分 | `不接入` |
| `GET` | `/3/movie/changes` | 最近变化的 movie ID 列表 | `预留`，批量增量刷新 |
| `GET` | `/3/movie/latest` | 最新创建的 TMDB movie ID | `预留` |
| `GET` | `/3/movie/now_playing` | 正在上映 | `预留` |
| `GET` | `/3/movie/popular` | 热门电影 | `预留` |
| `GET` | `/3/movie/top_rated` | 高分电影 | `预留` |
| `GET` | `/3/movie/upcoming` | 即将上映 | `预留` |

Movie details 当前官方字段包括：`id`、`title`、`original_title`、`release_date`、`overview`、`tagline`、`runtime`、`genres`、`production_companies`、`production_countries`、`spoken_languages`、`origin_country`、`belongs_to_collection`、`status`、`budget`、`revenue`、`homepage`、`imdb_id`、`poster_path`、`backdrop_path`、`popularity`、`vote_average`、`vote_count` 等。

### 11.3 TV Series

| Method | Path | 能力 | Mova 规划 |
| --- | --- | --- | --- |
| `GET` | `/3/tv/{series_id}` | TV 完整详情和 season summaries | `当前`，目标 `核心` |
| `GET` | `/3/tv/{series_id}/alternative_titles` | TV 别名 | `核心`，只用于严格名称验证 |
| `GET` | `/3/tv/{series_id}/aggregate_credits` | 全部季集聚合 cast/crew | `当前`，目标在 Mova 条目详情中按需获取 |
| `GET` | `/3/tv/{series_id}/credits` | 最新一季 cast/crew | `预留`，默认使用 aggregate credits |
| `GET` | `/3/tv/{series_id}/external_ids` | IMDb、TVDB、Wikidata、社交 ID | `当前`通过 TV details append 获取并持久化全部支持字段 |
| `GET` | `/3/tv/{series_id}/images` | `posters / backdrops / logos` 集合 | `核心`，目标通过 append 获取 |
| `GET` | `/3/tv/{series_id}/content_ratings` | 各地区内容分级 | `核心`，目标通过 append 获取 |
| `GET` | `/3/tv/{series_id}/episode_groups` | DVD、流媒体或其它集序分组 | `预留`，处理非标准播放顺序 |
| `GET` | `/3/tv/{series_id}/screened_theatrically` | 院线上映过的季集 | `预留` |
| `GET` | `/3/tv/{series_id}/keywords` | TV 关键词 | `预留` |
| `GET` | `/3/tv/{series_id}/translations` | TV 翻译集合 | `预留` |
| `GET` | `/3/tv/{series_id}/videos` | TV 视频 | `预留` |
| `GET` | `/3/tv/{series_id}/recommendations` | 推荐 TV | `预留` |
| `GET` | `/3/tv/{series_id}/similar` | 相似 TV | `预留` |
| `GET` | `/3/tv/{series_id}/reviews` | 评论 | `预留` |
| `GET` | `/3/tv/{series_id}/lists` | 所属 TMDB 列表 | `预留` |
| `GET` | `/3/tv/{series_id}/watch/providers` | 分地区观看渠道 | `预留`，使用时标注 JustWatch |
| `GET` | `/3/tv/{series_id}/changes` | 单剧近期变更 | `预留`，增量元数据刷新 |
| `GET` | `/3/tv/{series_id}/account_states` | TMDB 账户状态 | `不接入` |
| `POST/DELETE` | `/3/tv/{series_id}/rating` | 写入/删除 TMDB 用户评分 | `不接入` |
| `GET` | `/3/tv/changes` | 最近变化的 TV ID | `预留` |
| `GET` | `/3/tv/latest` | 最新创建的 TV ID | `预留` |
| `GET` | `/3/tv/airing_today` | 今日播出 | `预留` |
| `GET` | `/3/tv/on_the_air` | 未来 7 天播出 | `预留` |
| `GET` | `/3/tv/popular` | 热门 TV | `预留` |
| `GET` | `/3/tv/top_rated` | 高分 TV | `预留` |

TV details 当前官方字段包括：`id`、`name`、`original_name`、`first_air_date`、`last_air_date`、`overview`、`tagline`、`episode_run_time`、`number_of_seasons`、`number_of_episodes`、`created_by`、`networks`、`seasons`、`genres`、`production_companies`、`production_countries`、`origin_country`、`original_language`、`languages`、`spoken_languages`、`status`、`type`、`in_production`、`last_episode_to_air`、`next_episode_to_air`、`poster_path`、`backdrop_path`、`popularity`、`vote_average`、`vote_count` 等。

### 11.4 TV Season

| Method | Path | 能力 | Mova 规划 |
| --- | --- | --- | --- |
| `GET` | `/3/tv/{series_id}/season/{season_number}` | 季详情及本季 episodes | `当前`，目标只拉本地存在季 |
| `GET` | `/3/tv/{series_id}/season/{season_number}/aggregate_credits` | 本季聚合 cast/crew | `预留` |
| `GET` | `/3/tv/{series_id}/season/{season_number}/credits` | 本季 credits | `预留` |
| `GET` | `/3/tv/{series_id}/season/{season_number}/external_ids` | TVDB/Wikidata 等季 ID | `预留` |
| `GET` | `/3/tv/{series_id}/season/{season_number}/images` | 季 `posters` 集合 | `核心`，后续季海报选择 |
| `GET` | `/3/tv/{series_id}/season/{season_number}/translations` | 季翻译 | `预留` |
| `GET` | `/3/tv/{series_id}/season/{season_number}/videos` | 季视频 | `预留` |
| `GET` | `/3/tv/{series_id}/season/{season_number}/watch/providers` | 季观看渠道 | `预留`，使用时标注 JustWatch |
| `GET` | `/3/tv/{series_id}/season/{season_number}/account_states` | TMDB 账户状态 | `不接入` |
| `GET` | `/3/tv/season/{season_id}/changes` | 季近期变更 | `预留` |

Season details 包含 `id`、`season_number`、`name`、`air_date`、`overview`、`poster_path`、`vote_average`、`networks` 和 episodes。每个 episode 还包含 `id`、`episode_number`、`episode_type`、`air_date`、`name`、`overview`、`runtime`、`still_path`、`vote_average`、`vote_count`、crew 和 guest stars。

### 11.5 TV Episode 与 Episode Group

| Method | Path | 能力 | Mova 规划 |
| --- | --- | --- | --- |
| `GET` | `/3/tv/{series_id}/season/{season_number}/episode/{episode_number}` | 单集完整详情 | `预留`，季详情不足时按需使用 |
| `GET` | `/3/tv/{series_id}/season/{season_number}/episode/{episode_number}/credits` | 单集 cast/crew | `预留` |
| `GET` | `/3/tv/{series_id}/season/{season_number}/episode/{episode_number}/external_ids` | IMDb、TVDB、Wikidata 单集 ID | `预留` |
| `GET` | `/3/tv/{series_id}/season/{season_number}/episode/{episode_number}/images` | 单集 `stills` 集合 | `核心`，后续剧照选择 |
| `GET` | `/3/tv/{series_id}/season/{season_number}/episode/{episode_number}/translations` | 单集翻译 | `预留` |
| `GET` | `/3/tv/{series_id}/season/{season_number}/episode/{episode_number}/videos` | 单集视频 | `预留` |
| `GET` | `/3/tv/{series_id}/season/{season_number}/episode/{episode_number}/account_states` | TMDB 账户状态 | `不接入` |
| `POST/DELETE` | `/3/tv/{series_id}/season/{season_number}/episode/{episode_number}/rating` | 写入/删除 TMDB 单集评分 | `不接入` |
| `GET` | `/3/tv/episode/{episode_id}/changes` | 单集近期变更 | `预留` |
| `GET` | `/3/tv/episode_group/{tv_episode_group_id}` | 自定义 episode group 详情 | `预留`，DVD/绝对集序 |

### 11.6 Person 与 Credit

| Method | Path | 能力 | Mova 规划 |
| --- | --- | --- | --- |
| `GET` | `/3/person/{person_id}` | 人物详情 | `按需`，演员详情页 |
| `GET` | `/3/person/{person_id}/combined_credits` | 电影和 TV 合并作品表 | `预留` |
| `GET` | `/3/person/{person_id}/movie_credits` | 电影作品表 | `预留` |
| `GET` | `/3/person/{person_id}/tv_credits` | TV 作品表 | `预留` |
| `GET` | `/3/person/{person_id}/external_ids` | IMDb/Wikidata/社交 ID | `预留` |
| `GET` | `/3/person/{person_id}/images` | `profiles` 头像集合 | `按需` |
| `GET` | `/3/person/{person_id}/tagged_images` | 标记人物的剧照 | `预留` |
| `GET` | `/3/person/{person_id}/translations` | 人物传记翻译 | `预留` |
| `GET` | `/3/person/{person_id}/changes` | 人物近期变更 | `预留` |
| `GET` | `/3/person/popular` | 热门人物 | `预留` |
| `GET` | `/3/person/latest` | 最新人物 ID | `预留` |
| `GET` | `/3/person/changes` | 最近变化的人物 ID | `预留` |
| `GET` | `/3/credit/{credit_id}` | 单条演职员 credit 详情 | `预留` |

Person details 包含 `id`、`name`、`also_known_as`、`biography`、`birthday`、`deathday`、`gender`、`place_of_birth`、`known_for_department`、`profile_path`、`imdb_id`、`homepage` 和 popularity。

### 11.7 Collection、Company、Network、Keyword 与 Review

| Method | Path | 能力 | Mova 规划 |
| --- | --- | --- | --- |
| `GET` | `/3/collection/{collection_id}` | 电影合集详情和 parts | `预留`，电影合集页 |
| `GET` | `/3/collection/{collection_id}/images` | 合集 posters/backdrops | `预留` |
| `GET` | `/3/collection/{collection_id}/translations` | 合集翻译 | `预留` |
| `GET` | `/3/company/{company_id}` | 制作公司详情 | `预留` |
| `GET` | `/3/company/{company_id}/alternative_names` | 公司别名 | `预留` |
| `GET` | `/3/company/{company_id}/images` | 公司 `logos` 集合，含 PNG/SVG 类型 | `预留` |
| `GET` | `/3/network/{network_id}` | 电视网详情 | `预留` |
| `GET` | `/3/network/{network_id}/alternative_names` | 电视网别名 | `预留` |
| `GET` | `/3/network/{network_id}/images` | 电视网 `logos` 集合，含 PNG/SVG 类型 | `预留` |
| `GET` | `/3/keyword/{keyword_id}` | 关键词详情 | `预留` |
| `GET` | `/3/keyword/{keyword_id}/movies` | 使用关键词的电影 | `预留` |
| `GET` | `/3/review/{review_id}` | 单条评论详情 | `预留` |

### 11.8 TMDB 用户账户、会话、列表和评分写入

Mova 使用自己的账户、继续观看、评分和列表模型，不把用户身份代理到 TMDB。因此以下接口完整记录但默认不接入：

| Method | Path | 能力 |
| --- | --- | --- |
| `GET` | `/3/account/{account_id}` | TMDB 账户详情 |
| `POST` | `/3/account/{account_id}/favorite` | 收藏 movie/TV |
| `POST` | `/3/account/{account_id}/watchlist` | 加入 watchlist |
| `GET` | `/3/account/{account_id}/favorite/movies` | 收藏电影 |
| `GET` | `/3/account/{account_id}/favorite/tv` | 收藏 TV |
| `GET` | `/3/account/{account_id}/lists` | 用户列表 |
| `GET` | `/3/account/{account_id}/rated/movies` | 已评分电影 |
| `GET` | `/3/account/{account_id}/rated/tv` | 已评分 TV |
| `GET` | `/3/account/{account_id}/rated/tv/episodes` | 已评分单集 |
| `GET` | `/3/account/{account_id}/watchlist/movies` | 电影 watchlist |
| `GET` | `/3/account/{account_id}/watchlist/tv` | TV watchlist |
| `GET` | `/3/authentication/guest_session/new` | 创建 guest session |
| `GET` | `/3/authentication/token/new` | 创建 request token |
| `POST` | `/3/authentication/token/validate_with_login` | 用户名密码验证 request token |
| `POST` | `/3/authentication/session/new` | 创建 session |
| `POST` | `/3/authentication/session/convert/4` | 从 v4 token 创建 v3 session |
| `DELETE` | `/3/authentication/session` | 删除 session |
| `GET` | `/3/guest_session/{guest_session_id}/rated/movies` | guest 已评分电影 |
| `GET` | `/3/guest_session/{guest_session_id}/rated/tv` | guest 已评分 TV |
| `GET` | `/3/guest_session/{guest_session_id}/rated/tv/episodes` | guest 已评分单集 |
| `POST` | `/3/list` | 创建 TMDB list |
| `GET/DELETE` | `/3/list/{list_id}` | 查询/删除 list |
| `POST` | `/3/list/{list_id}/add_item` | 添加电影到 list |
| `POST` | `/3/list/{list_id}/remove_item` | 从 list 删除电影 |
| `POST` | `/3/list/{list_id}/clear` | 清空 list |
| `GET` | `/3/list/{list_id}/item_status` | 查询电影是否在 list |
