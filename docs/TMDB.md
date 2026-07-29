# TMDB v3 API 参考

本文依据 TMDB 官方 v3 OpenAPI 提供项目内的中文 endpoint 索引，并标明 Mova 的采用状态。TMDB 官方文档和 OpenAPI 是请求参数、响应结构及接口增删的上游事实来源；本文不复制完整 schema。

Mova 的类型路由、严格匹配、字段覆盖、图片、缓存和失败策略见 [`TMDB_INTEGRATION.md`](TMDB_INTEGRATION.md)。媒体库扫描编排见 [`MEDIA_LIBRARY_SCAN.md`](MEDIA_LIBRARY_SCAN.md)。

## 官方入口

| 类型 | 地址 |
| --- | --- |
| 开发者文档 | <https://developer.themoviedb.org/> |
| v3 API 请求基址 | `https://api.themoviedb.org/3` |
| v3 OpenAPI | <https://developer.themoviedb.org/openapi/tmdb-api.json> |
| v4 开发者文档 | <https://developer.themoviedb.org/v4/docs/getting-started> |
| API Read Access Token | <https://www.themoviedb.org/settings/api> |
| 服务状态 | <https://status.themoviedb.org/> |

本文只覆盖官方 v3 OpenAPI。Mova 当前不代理 TMDB 用户账户，也不使用 v4 list API；未来接入 v4 时应建立独立参考，避免混合两个版本的认证和资源模型。

## 采用状态

本目录按 2026-07-28 的官方 OpenAPI 核对，覆盖全部 148 个 path、152 个 operation。

- `当前`：现有 Rust 代码已经调用。
- `规划`：已有明确产品需求，但尚未成为当前运行时能力。
- `按需`：只在对应详情或功能中加载。
- `预留`：保留为未来能力索引。
- `不接入`：依赖 TMDB 用户账户、属于写操作，或与 Mova 自托管模型重复。

“规划”“按需”或“预留”不表示接口已经可由 Mova 客户端使用。当前实现以 [`TMDB_INTEGRATION.md`](TMDB_INTEGRATION.md) 为准。

## 1. 基础配置、查找、搜索与发现

| Method | Path | 能力 | Mova 状态 |
| --- | --- | --- | --- |
| `GET` | `/3/authentication` | 验证 API key/token | `规划`，服务配置检查 |
| `GET` | `/3/configuration` | 图片 base URL 和有效尺寸 | `规划` |
| `GET` | `/3/configuration/countries` | ISO 3166-1 国家列表 | `预留` |
| `GET` | `/3/configuration/jobs` | 演职员部门和 job 列表 | `预留` |
| `GET` | `/3/configuration/languages` | ISO 639-1 语言列表 | `预留` |
| `GET` | `/3/configuration/primary_translations` | TMDB 主要翻译语言 | `预留` |
| `GET` | `/3/configuration/timezones` | 国家与时区映射 | `预留` |
| `GET` | `/3/certification/movie/list` | 电影分级体系 | `预留` |
| `GET` | `/3/certification/tv/list` | TV 分级体系 | `预留` |
| `GET` | `/3/genre/movie/list` | 电影 genre 字典 | `预留` |
| `GET` | `/3/genre/tv/list` | TV genre 字典 | `预留` |
| `GET` | `/3/find/{external_id}` | 用外部 ID 反查 TMDB | `预留`，NFO 非 TMDB ID 对接 |
| `GET` | `/3/search/movie` | 搜索电影 | `当前` |
| `GET` | `/3/search/tv` | 搜索 TV | `当前` |
| `GET` | `/3/search/multi` | 同时搜索 movie、TV、person | `不接入`，违反单类型路由 |
| `GET` | `/3/search/person` | 搜索人物 | `预留` |
| `GET` | `/3/search/collection` | 搜索电影合集 | `预留` |
| `GET` | `/3/search/company` | 搜索制作公司 | `预留` |
| `GET` | `/3/search/keyword` | 搜索关键词 | `预留` |
| `GET` | `/3/discover/movie` | 多条件发现电影 | `预留` |
| `GET` | `/3/discover/tv` | 多条件发现 TV | `预留` |
| `GET` | `/3/trending/all/{time_window}` | 全类型趋势 | `预留` |
| `GET` | `/3/trending/movie/{time_window}` | 电影趋势 | `预留` |
| `GET` | `/3/trending/tv/{time_window}` | TV 趋势 | `预留` |
| `GET` | `/3/trending/person/{time_window}` | 人物趋势 | `预留` |
| `GET` | `/3/watch/providers/regions` | 流媒体 provider 可用地区 | `预留` |
| `GET` | `/3/watch/providers/movie` | 电影 provider 字典 | `预留`，使用时遵守 JustWatch attribution |
| `GET` | `/3/watch/providers/tv` | TV provider 字典 | `预留`，使用时遵守 JustWatch attribution |

## 2. Movie

| Method | Path | 能力 | Mova 状态 |
| --- | --- | --- | --- |
| `GET` | `/3/movie/{movie_id}` | 电影完整详情 | `当前` |
| `GET` | `/3/movie/{movie_id}/alternative_titles` | 各地区别名 | `当前`，仅用于严格名称验证 |
| `GET` | `/3/movie/{movie_id}/credits` | 电影 cast/crew | `当前`，演员按需获取 |
| `GET` | `/3/movie/{movie_id}/external_ids` | IMDb、Wikidata、社交 ID | `当前`，通过 details append |
| `GET` | `/3/movie/{movie_id}/images` | posters、backdrops、logos | `当前`，通过 details append |
| `GET` | `/3/movie/{movie_id}/release_dates` | 地区发行日期和分级 | `规划` |
| `GET` | `/3/movie/{movie_id}/keywords` | 电影关键词 | `预留` |
| `GET` | `/3/movie/{movie_id}/translations` | 标题和简介翻译 | `预留` |
| `GET` | `/3/movie/{movie_id}/videos` | 预告片和花絮 | `预留` |
| `GET` | `/3/movie/{movie_id}/recommendations` | 推荐电影 | `预留` |
| `GET` | `/3/movie/{movie_id}/similar` | 相似电影 | `预留` |
| `GET` | `/3/movie/{movie_id}/reviews` | 用户评论 | `预留` |
| `GET` | `/3/movie/{movie_id}/lists` | 包含该电影的 TMDB 列表 | `预留` |
| `GET` | `/3/movie/{movie_id}/watch/providers` | 地区观看渠道 | `预留` |
| `GET` | `/3/movie/{movie_id}/changes` | 单片近期变更 | `预留` |
| `GET` | `/3/movie/{movie_id}/account_states` | TMDB 账户状态 | `不接入` |
| `POST/DELETE` | `/3/movie/{movie_id}/rating` | 写入或删除 TMDB 用户评分 | `不接入` |
| `GET` | `/3/movie/changes` | 最近变化的 movie ID | `预留` |
| `GET` | `/3/movie/latest` | 最新创建的 movie ID | `预留` |
| `GET` | `/3/movie/now_playing` | 正在上映 | `预留` |
| `GET` | `/3/movie/popular` | 热门电影 | `预留` |
| `GET` | `/3/movie/top_rated` | 高分电影 | `预留` |
| `GET` | `/3/movie/upcoming` | 即将上映 | `预留` |

## 3. TV Series

| Method | Path | 能力 | Mova 状态 |
| --- | --- | --- | --- |
| `GET` | `/3/tv/{series_id}` | TV 完整详情和 season summaries | `当前` |
| `GET` | `/3/tv/{series_id}/alternative_titles` | TV 别名 | `当前`，仅用于严格名称验证 |
| `GET` | `/3/tv/{series_id}/aggregate_credits` | 全部季集聚合 cast/crew | `当前`，演员按需获取 |
| `GET` | `/3/tv/{series_id}/credits` | 最新一季 cast/crew | `预留` |
| `GET` | `/3/tv/{series_id}/external_ids` | IMDb、TVDB、Wikidata、社交 ID | `当前`，通过 details append |
| `GET` | `/3/tv/{series_id}/images` | posters、backdrops、logos | `当前`，通过 details append |
| `GET` | `/3/tv/{series_id}/content_ratings` | 地区内容分级 | `规划` |
| `GET` | `/3/tv/{series_id}/episode_groups` | 非标准集序分组 | `预留` |
| `GET` | `/3/tv/{series_id}/screened_theatrically` | 院线上映过的季集 | `预留` |
| `GET` | `/3/tv/{series_id}/keywords` | TV 关键词 | `预留` |
| `GET` | `/3/tv/{series_id}/translations` | TV 翻译集合 | `预留` |
| `GET` | `/3/tv/{series_id}/videos` | TV 视频 | `预留` |
| `GET` | `/3/tv/{series_id}/recommendations` | 推荐 TV | `预留` |
| `GET` | `/3/tv/{series_id}/similar` | 相似 TV | `预留` |
| `GET` | `/3/tv/{series_id}/reviews` | 评论 | `预留` |
| `GET` | `/3/tv/{series_id}/lists` | 所属 TMDB 列表 | `预留` |
| `GET` | `/3/tv/{series_id}/watch/providers` | 地区观看渠道 | `预留` |
| `GET` | `/3/tv/{series_id}/changes` | 单剧近期变更 | `预留` |
| `GET` | `/3/tv/{series_id}/account_states` | TMDB 账户状态 | `不接入` |
| `POST/DELETE` | `/3/tv/{series_id}/rating` | 写入或删除 TMDB 用户评分 | `不接入` |
| `GET` | `/3/tv/changes` | 最近变化的 TV ID | `预留` |
| `GET` | `/3/tv/latest` | 最新创建的 TV ID | `预留` |
| `GET` | `/3/tv/airing_today` | 今日播出 | `预留` |
| `GET` | `/3/tv/on_the_air` | 未来 7 天播出 | `预留` |
| `GET` | `/3/tv/popular` | 热门 TV | `预留` |
| `GET` | `/3/tv/top_rated` | 高分 TV | `预留` |

## 4. TV Season

| Method | Path | 能力 | Mova 状态 |
| --- | --- | --- | --- |
| `GET` | `/3/tv/{series_id}/season/{season_number}` | 季详情及本季 episodes | `当前` |
| `GET` | `/3/tv/{series_id}/season/{season_number}/aggregate_credits` | 本季聚合 cast/crew | `预留` |
| `GET` | `/3/tv/{series_id}/season/{season_number}/credits` | 本季 credits | `预留` |
| `GET` | `/3/tv/{series_id}/season/{season_number}/external_ids` | 季外部 ID | `预留` |
| `GET` | `/3/tv/{series_id}/season/{season_number}/images` | 季 posters 集合 | `规划` |
| `GET` | `/3/tv/{series_id}/season/{season_number}/translations` | 季翻译 | `预留` |
| `GET` | `/3/tv/{series_id}/season/{season_number}/videos` | 季视频 | `预留` |
| `GET` | `/3/tv/{series_id}/season/{season_number}/watch/providers` | 季观看渠道 | `预留` |
| `GET` | `/3/tv/{series_id}/season/{season_number}/account_states` | TMDB 账户状态 | `不接入` |
| `GET` | `/3/tv/season/{season_id}/changes` | 季近期变更 | `预留` |

## 5. TV Episode 与 Episode Group

| Method | Path | 能力 | Mova 状态 |
| --- | --- | --- | --- |
| `GET` | `/3/tv/{series_id}/season/{season_number}/episode/{episode_number}` | 单集完整详情 | `预留` |
| `GET` | `/3/tv/{series_id}/season/{season_number}/episode/{episode_number}/credits` | 单集 cast/crew | `预留` |
| `GET` | `/3/tv/{series_id}/season/{season_number}/episode/{episode_number}/external_ids` | 单集外部 ID | `预留` |
| `GET` | `/3/tv/{series_id}/season/{season_number}/episode/{episode_number}/images` | 单集 stills 集合 | `规划` |
| `GET` | `/3/tv/{series_id}/season/{season_number}/episode/{episode_number}/translations` | 单集翻译 | `预留` |
| `GET` | `/3/tv/{series_id}/season/{season_number}/episode/{episode_number}/videos` | 单集视频 | `预留` |
| `GET` | `/3/tv/{series_id}/season/{season_number}/episode/{episode_number}/account_states` | TMDB 账户状态 | `不接入` |
| `POST/DELETE` | `/3/tv/{series_id}/season/{season_number}/episode/{episode_number}/rating` | 写入或删除单集评分 | `不接入` |
| `GET` | `/3/tv/episode/{episode_id}/changes` | 单集近期变更 | `预留` |
| `GET` | `/3/tv/episode_group/{tv_episode_group_id}` | 自定义 episode group 详情 | `预留` |

## 6. Person 与 Credit

| Method | Path | 能力 | Mova 状态 |
| --- | --- | --- | --- |
| `GET` | `/3/person/{person_id}` | 人物详情 | `按需` |
| `GET` | `/3/person/{person_id}/combined_credits` | 电影和 TV 合并作品表 | `预留` |
| `GET` | `/3/person/{person_id}/movie_credits` | 电影作品表 | `预留` |
| `GET` | `/3/person/{person_id}/tv_credits` | TV 作品表 | `预留` |
| `GET` | `/3/person/{person_id}/external_ids` | 人物外部 ID | `预留` |
| `GET` | `/3/person/{person_id}/images` | profiles 头像集合 | `按需` |
| `GET` | `/3/person/{person_id}/tagged_images` | 标记人物的剧照 | `预留` |
| `GET` | `/3/person/{person_id}/translations` | 人物传记翻译 | `预留` |
| `GET` | `/3/person/{person_id}/changes` | 人物近期变更 | `预留` |
| `GET` | `/3/person/popular` | 热门人物 | `预留` |
| `GET` | `/3/person/latest` | 最新人物 ID | `预留` |
| `GET` | `/3/person/changes` | 最近变化的人物 ID | `预留` |
| `GET` | `/3/credit/{credit_id}` | 单条演职员 credit 详情 | `预留` |

## 7. Collection、Company、Network、Keyword 与 Review

| Method | Path | 能力 | Mova 状态 |
| --- | --- | --- | --- |
| `GET` | `/3/collection/{collection_id}` | 电影合集详情和 parts | `预留` |
| `GET` | `/3/collection/{collection_id}/images` | 合集 posters/backdrops | `预留` |
| `GET` | `/3/collection/{collection_id}/translations` | 合集翻译 | `预留` |
| `GET` | `/3/company/{company_id}` | 制作公司详情 | `预留` |
| `GET` | `/3/company/{company_id}/alternative_names` | 公司别名 | `预留` |
| `GET` | `/3/company/{company_id}/images` | 公司 logos 集合 | `预留` |
| `GET` | `/3/network/{network_id}` | 电视网详情 | `预留` |
| `GET` | `/3/network/{network_id}/alternative_names` | 电视网别名 | `预留` |
| `GET` | `/3/network/{network_id}/images` | 电视网 logos 集合 | `预留` |
| `GET` | `/3/keyword/{keyword_id}` | 关键词详情 | `预留` |
| `GET` | `/3/keyword/{keyword_id}/movies` | 使用关键词的电影 | `预留` |
| `GET` | `/3/review/{review_id}` | 单条评论详情 | `预留` |

## 8. TMDB 用户账户、会话、列表和评分写入

Mova 使用自己的账户、继续观看、评分和列表模型，不把用户身份代理到 TMDB。以下接口默认不接入：

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
| `GET/DELETE` | `/3/list/{list_id}` | 查询或删除 list |
| `POST` | `/3/list/{list_id}/add_item` | 添加电影到 list |
| `POST` | `/3/list/{list_id}/remove_item` | 从 list 删除电影 |
| `POST` | `/3/list/{list_id}/clear` | 清空 list |
| `GET` | `/3/list/{list_id}/item_status` | 查询电影是否在 list |
