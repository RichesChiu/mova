# Mova Emby 兼容接口

此接口让第三方播放器使用 Emby 连接方式浏览和播放 Mova 媒体。兼容范围为连接浏览和直播放续播两个阶段，处于实验状态。爆米花 macOS、iOS、安卓手机和安卓电视均是验收目标；实机通过情况单独记录，不能由 HTTP 测试代替。

## 实现边界与分步验收

兼容层负责 Emby 路由、鉴权形式和字段转换，调用 application 用例；SQL 留在 db，现有原生 API 保持契约版本 1。

| 能力 | 复用内容 | 新增内容 |
| --- | --- | --- |
| 连接、登录 | 账户密码校验、限流、权限、会话撤销 | 稳定服务身份、设备元数据、Emby token 提取与登录响应 |
| 媒体库、电影、季集、图片 | 现有媒体实体、可用文件、元数据、图库和访问权限 | 协议 ID、DTO、任意偏移分页与精确单集续播查询 |
| 本地播放 | 原有文件读取、Range、HEAD、字幕转换 | PlaybackInfo、媒体源映射及播放 URL |
| STRM 播放 | 原有 HTTP/HTTPS 代理及目标、重定向、响应校验 | STRM 媒体源映射，保留未知时长和编码 |
| 进度、续播 | playback_progress、continue_watching、资源通知 | ticks 转换、播放会话、停止后迟到事件防护 |

可以分步发布实验版本：第一阶段验收连接、登录、库列表、详情和季集；第二阶段增加本地及 STRM 的播放、拖动、停止和续播，构成播放 MVP。第一阶段本身不算播放 MVP。当前代码覆盖这两个阶段的接口；每个平台需分别完成实机闭环，收藏、NextUp、转码等扩展可独立排期，不要求补全整个 Emby API。

## 入口与身份

- 标准入口为 `http[s]://host[:port]/emby`，同名根路径是别名。生成的媒体 URL 使用 `/emby`。
- 只提供 JSON；成功响应为协议对象、数组或 `Items / TotalRecordCount / StartIndex`。错误使用实际 HTTP 状态及 `ResponseStatus.ErrorCode / Message`；`Retry-After`、`Content-Range` 等传输头保留。
- `ServerName`、`ProductName` 为 `Mova`。`Version` 为兼容标识 `4.8.0.0`，不表示运行着 Emby Server；`MovaVersion` 是实际构建版本。
- `ServerId` 持久化在数据库中，重启后稳定。恢复同一数据库保留身份；独立服务器不能共用同一数据库备份的身份。
- 资源 ID 是 32 位十六进制字符串，前半部分区分用户、库、季、条目、文件和根节点，后半部分编码内部 ID。客户端应将其视为不透明字符串。重复扫描同一实体保持 ID；条目被合并删除后，旧 ID 可返回 404，需要刷新列表。
- `ItemId` 表示电影或具体单集；`MediaSourceId` 表示属于该条目的具体文件版本。请求不能跨条目引用文件。

## 认证

`POST /Users/AuthenticateByName` 接受 `Username` 和 `Pw`；也接受对应小写字段及 `Password`。返回 `User`、`AccessToken`、`SessionInfo` 和 `ServerId`。

支持以下凭据形式：

- `X-Emby-Token` 或 `X-MediaBrowser-Token`。
- `Authorization` 或 `X-Emby-Authorization` 中的 `Emby` / `MediaBrowser` 参数格式，使用 `Token` 字段；`Authorization: Bearer` 也可使用兼容 token。
- 查询参数 `api_key` 或 `X-Emby-Token`，供图片、视频和字幕使用。

同一次请求提供不同 token 时拒绝认证。兼容入口只接受兼容登录签发的 token。客户端元数据通过授权头中的 `Client / Device / DeviceId / Version` 记录；它们不替代凭据。不得记录完整授权头或带 token 的原始 URL。

设备 token 有效期 90 天，无 refresh 流程；过期返回 401，需要重新登录。凭据只保存 SHA-256 哈希，设备记录依附现有账户会话。注销删除当前会话，改密、停用、删除账户和既有会话清理同步生效。登录失败与原生客户端共用账户限流。`Users/Public` 返回空数组，客户端使用账户密码输入登录。

路径或参数中的 `UserId` 必须与认证用户相同。普通用户只看到获准访问的媒体库；图片、字幕、视频和播放事件再次验证权限。JSON 不缓存，避免缓存携带 token 的媒体 URL。

## 路由清单

以下路径省略 `/emby` 前缀。`GET` 资源也支持 HTTP HEAD 的标准响应头行为。

| 方法与路径 | 返回或行为 |
| --- | --- |
| GET `/System/Info/Public` | 匿名服务信息 |
| GET `/System/Info` | 登录后的服务信息 |
| GET、POST `/System/Ping` | `Mova` 文本 |
| GET `/Users/Public` | 空数组 |
| POST `/Users/AuthenticateByName` | 登录结果 |
| GET `/Users/Me`、`/Users/{user}` | 当前用户 |
| POST `/Sessions/Logout` | 撤销当前 token，204 |
| POST `/Sessions/Capabilities`、`/Sessions/Capabilities/Full` | 接收客户端能力声明，204；不提供远程控制 |
| GET `/Users/{user}/Views` | 可见媒体库分页结果 |
| GET `/Users/{user}/Items/Root` | 虚拟根目录 |
| GET `/Items`、`/Users/{user}/Items` | 条目分页结果 |
| GET `/Items/{item}`、`/Users/{user}/Items/{item}` | 单个条目、季或库 |
| GET `/Shows/{item}/Seasons` | 有可用本地文件的季 |
| GET `/Shows/{item}/Episodes` | 有可用本地文件的集 |
| GET `/Users/{user}/Items/Latest` | 最近添加的条目数组 |
| GET `/Users/{user}/Items/Resume` | 有进度且未完成的电影或单集分页结果 |
| GET `/Items/{item}/Images/{kind}`、`/Items/{item}/Images/{kind}/{index}` | Primary、Backdrop、Logo；季支持前两种；索引仅支持 0 |
| GET、POST `/Items/{item}/PlaybackInfo` | 选中的媒体源、播放会话，或 NoCompatibleStream |
| GET、HEAD `/Videos/{item}/stream`、`/Videos/{item}/stream.{container}`、`/Video/{item}/stream` | 原文件或 STRM 代理；扩展名别名不会触发格式转换 |
| GET、HEAD `/Videos/{item}/{source}/Subtitles/{index}/Stream.vtt` | 复用字幕转换与缓存，输出 WebVTT |
| POST `/Sessions/Playing`、`/Sessions/Playing/Progress`、`/Sessions/Playing/Stopped` | 播放事件，204 |

## 浏览语义

查询参数名称不区分大小写。支持 `UserId`、`ParentId`、`SeasonId`、`Recursive`、`IncludeItemTypes`、`Ids`、`SearchTerm`、`StartIndex`、`Limit`、`SortBy`、`SortOrder`、`Filters`。

- `StartIndex` 为任意非负偏移；`Limit` 为 0–500，默认 100。总数在用户权限和筛选条件之后、分页之前计算。
- `IncludeItemTypes` 支持 Movie、Series、Episode。普通库列表默认 Movie/Series；递归查询默认包含 Episode。Season 父级只返回本季集数。Series 父级非递归且未指定类型/搜索时返回季列表。
- `SortBy` 支持 Name、SortName、DateCreated、DatePlayed、ProductionYear、PremiereDate、IndexNumber、ParentIndexNumber；可用逗号指定多个字段。`SortOrder` 支持 Ascending、Descending；最终使用条目 ID 保证排序稳定。
- `Filters` 支持 IsResumable、IsNotFolder。未实现的筛选值和排序字段返回 400，避免返回貌似成功的错误结果。
- `Fields` 等展示提示不用于裁剪必需数据；响应提供基础元数据、媒体源和用户状态。分页结果通过批量查询读取文件、音轨、字幕、评分和进度。
- 媒体库以混合 CollectionFolder 表达，CollectionType 为 null。具体内容由 Type 区分。
- 远端全集大纲不参与可播放集列表。没有文件的历史集条目不会作为可播放结果返回。
- Latest 默认返回 Movie/Episode，按入库时间倒序；Resume 默认按最后播放时间倒序，返回具体单集，不复用最多 20 部剧的原生首页队列。

## 播放与 STRM

`PlaybackInfo` 接受 UserId、MediaSourceId、EnableDirectPlay、EnableDirectStream、MaxStreamingBitrate，以及 DeviceProfile.DirectPlayProfiles 中的视频容器和音视频编码列表。POST 的标量参数优先于同名查询参数。未指定媒体源时，按用户最近选中的有效版本回退到首个现存版本。

只提供原文件播放。没有可用原文件路径或客户端条件不允许直播放时，返回对应错误；能力不匹配时为 `MediaSources: []` 和 `ErrorCode: NoCompatibleStream`。不产生 TranscodingUrl，不声明支持转码。DeviceProfile 的复杂编码条件、容器转换、音频转码、字幕烧录和 HLS 属于未支持范围。

本地文件复用 Range/HEAD 实现。静态输出保留全部内嵌轨道，原生播放器自行选择音轨；这里不触发音轨 remux。已有音轨索引直接映射；视频流索引未持久化，因此不虚构索引。字幕交付 URL 输出 VTT，字幕索引只在对应媒体源内有效。

STRM 仅接受 HTTP/HTTPS 直接媒体地址，继续复用目标校验、私网白名单、重定向限制、并发控制和响应验证。原始上游地址及其凭据不放入客户端 JSON。不能保证上游支持 Range；不支持非零 Range 时保留现有错误状态。载体的文件大小不是远端视频大小；未知的时长、容器、编码和码率不伪造。STRM 的内嵌轨道由播放器探测，不在服务端发现或抽取；本地外挂字幕使用现有字幕能力。

媒体 URL 使用相对 `/emby` 路径；反向代理应保留该前缀。额外路径前缀部署需要代理对生成 URL 做一致路由，不能只改登录入口。

## 进度与会话

`PlaybackInfo` 创建绑定当前 token、用户、条目和选中文件的 PlaySessionId。播放事件携带 ItemId、MediaSourceId、PlaySessionId、PositionTicks；允许通过 RunTimeTicks 或 Item.RunTimeTicks 提供未知片源时长。未提供 PlaySessionId 时复用最近匹配的会话；没有匹配会话，或收到 Playing 且最近会话已停止时创建。

- 1 秒为 10,000,000 ticks。落库向下取整到秒，负数和越界数值拒绝。
- 文件已知时长优先于客户端时长，位置不能超过时长。未知时长不自动标记完成；已知时长达到 95% 时完成。Stopped 本身不意味着看完。
- 会话在事件到达时续期 24 小时。停止后重复停止和迟到心跳不会覆盖进度；同一用户、条目以较新开始的兼容播放会话优先。
- 同一活动会话内按服务端接收顺序处理位置，允许用户向后拖动。不承诺识别客户端未提供序号的网络乱序。
- 进度使用现有 `(user_id, media_item_id)` 唯一记录，多文件版本共享进度。原生客户端可以读取同一结果；进度更新发送既有资源失效通知。
- 收藏、手动已看、NextUp、WebSocket、远程控制、下载管理和完整 Emby 管理 API 不在这两个阶段内。

## 存储与验证

迁移 `0011_emby_sessions.sql` 新增稳定服务身份、设备元数据和临时播放会话；设备外键引用现有 user_sessions，播放会话通过外键级联清理。永久观看记录继续使用 playback_progress。升级不要求重扫或重建缓存。

自动化验证覆盖登录/注销、改密撤销、权限、稳定服务 ID、分页、季集可用性、本地 Range/HEAD、播放事件、停止后迟到事件、重播、续播和已有数据库原地升级。STRM 通过受控 HTTP 上游验证 Range/HEAD、凭据隔离、未知时长进度、HLS 拒绝和受限目标拒绝；实际片源的可达性及 Range 行为仍需单独验收。

| 爆米花平台 | 本地电影/剧集 | STRM | 退出续播与重开登录 |
| --- | --- | --- | --- |
| macOS | 待实机验证 | 待实机验证 | 待实机验证 |
| iOS | 待实机验证 | 待实机验证 | 待实机验证 |
| 安卓手机 | 待实机验证 | 待实机验证 | 待实机验证 |
| 安卓电视 | 待实机验证 | 待实机验证 | 待实机验证 |

实机验收记录客户端版本、请求路径、失败状态和媒体类型；样本移除 token、账户和私有媒体路径。完成“连接、登录、浏览、选集、播放、拖动、退出、续播”后才能将相应平台标记通过。

## 协议参考

- [Emby 用户认证](https://dev.emby.media/doc/restapi/User-Authentication.html)
- [Emby 媒体库浏览](https://dev.emby.media/doc/restapi/Browsing-the-Library.html)
- [Emby 播放准备](https://dev.emby.media/reference/RestAPI/MediaInfoService/postItemsByIdPlaybackinfo.html)
- [Emby 播放事件](https://dev.emby.media/doc/restapi/Playback-Check-ins.html)
