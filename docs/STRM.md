# STRM（HTTP/HTTPS）媒体源设计说明

本文定义 Mova 对内容为单个 `http://` 或 `https://` URL 的 `.strm` 文件的当前扫描、存储、播放、安全和客户端契约。该能力目前标记为实验功能；RTSP、MMS、HLS、远程转码、文件监听和独立远程媒体服务不在此能力范围内。扫描、匹配、拖动播放或兼容性问题可通过 [GitHub Issues](https://github.com/RichesChiu/mova/issues) 或 [Telegram](https://t.me/mova_feedback) 反馈。

## 1. 目标

Mova 将 `.strm` 视为媒体库内的“远程媒体引用文件”而不是视频文件本体：

```text
/media/movies/Movie (2026)/Movie (2026).strm
```

文件内容：

```text
https://media.example.com/movies/movie.mp4?token=example
```

行为契约：

1. `.strm` 使用与本地电影、剧集相同的文件名、目录、NFO、图片和外挂字幕规则。
2. 扫描阶段只读取并校验引用文本，不访问远端，不运行 `ffprobe`。
3. TMDB 匹配只使用文件名、目录、NFO 等本地身份信息，绝不使用 URL 猜测标题。
4. 客户端继续请求 Mova 的 `/api/media-files/{id}/stream`，不接触远端 URL。
5. Mova 完成用户权限校验后，以流式、支持 Range 的方式代理远端 HTTP 响应。
6. 原始 URL 不写入数据库、不返回 API、不进入通知和普通日志。
7. STRM 与本地文件可以作为同一媒体条目的多个播放版本，并沿用现有播放进度与继续观看语义。
8. 一个非法或暂时不可达的 STRM 不能导致整个媒体库扫描失败。

## 2. 范围外能力

- `rtsp://`、`mms://`、`mmsh://`、`mmst://`
- `.m3u8`、HLS 清单和分片改写
- `file://`、绝对路径、UNC、SMB、WebDAV 客户端
- 多行播放列表、注释、请求头、Cookie 或独立认证配置
- 扫描阶段的远端探测、下载、缓存或 `ffprobe`
- STRM 内嵌音轨发现、音轨切换、内嵌字幕发现
- STRM 片头检测
- 远程转码、码率自适应或离线下载
- 向客户端返回 302 直连远端 URL
- 持久化远端可用性、Content-Length、ETag 等短期观测

遇到上述内容必须返回明确的不支持错误或记录扫描问题，不得静默降级成不受控行为。

## 3. 核心设计决策

实现的主要落点：

- `crates/mova-scan/src/discover.rs`：媒体扩展名、inventory、NFO/sidecar 和 `ffprobe` 分派。
- `crates/mova-application/src/scan_jobs.rs` 与 `scan_jobs/incremental.rs`：扫描流水线、增量计划、通知和最终路径对账。
- `crates/mova-db/src/media_items/query.rs` 与 `media_items/sync.rs`：媒体文件查询和事务同步。
- `apps/mova-server/src/handlers/media_files.rs`：鉴权、本地路径校验、Range 和音轨 remux。
- `apps/mova-server/src/response.rs`：`MediaFileResponse`。
- `apps/mova-web/src/api/types.ts`、`lib/media-file-details.ts`、`components/media-player-panel`：文件展示和播放。

全部 `media_files` 手写 SQL、`MediaFile` 构造点和公开响应都必须携带相同来源语义。

### 3.1 载体和媒体地址分离

- `media_files.file_path` 对本地文件仍表示视频路径。
- `media_files.file_path` 对 STRM 表示本地 `.strm` 载体路径。
- 数据库只保存引用内容的 SHA-256，不保存原始 URL。
- 播放请求每次重新读取载体文件并执行完整校验；这与本地视频被原地替换后按当前内容播放的语义一致。
- 当前内容哈希与最后扫描哈希不一致时不阻断播放；使用重新校验后的当前引用，数据库只在扫描事务中更新，播放 handler 不写媒体表。
- `file_size` 对 STRM 表示载体文件大小。Web 不得把该值展示成远端视频大小。

### 3.2 扫描和播放职责分开

- 扫描负责发现、语法校验、分组、NFO/sidecar、TMDB 匹配和持久化。
- 播放负责重新解析引用、访问控制、SSRF 防护、远端请求和响应代理。
- 远端暂时不可达属于播放时错误，不改变媒体条目和扫描匹配状态。

### 3.3 保持统一客户端契约

Web、macOS 和 iOS 均继续使用：

```http
GET /api/media-files/{id}/stream
HEAD /api/media-files/{id}/stream
```

客户端不根据协议拼接地址，也不解析 `.strm`。公开 API 只包含来源描述字段和稳定错误码，不暴露远端地址。

### 3.4 FFmpeg 网络能力保持关闭

HTTP/HTTPS 代理使用 Rust HTTP 客户端实现。`docker/base/runtime.Dockerfile` 中
FFmpeg 的 `--disable-network` 必须保留，STRM 必须跳过本地 `ffprobe`、音轨 remux 和片头检测。

## 4. 模块边界

仓库架构分工如下：

### `crates/mova-domain`

- 定义 `MediaSourceKind`：`LocalFile`、`Strm`。
- `MediaFile` 包含 `source_kind` 和 `stream_reference_hash`。
- 不包含 URL 解析、文件读取或 HTTP 类型。

### `crates/mova-scan`

- 识别 `.strm`。
- 提供有界读取和纯解析函数。
- 生成 STRM 引用哈希。
- 复用现有文件名、NFO、本地图片和外挂字幕逻辑。
- STRM 分支不调用 `probe_media_file_*`。
- 返回可恢复的单文件发现问题，不把非法 STRM 提升为整库 I/O 失败。

### `crates/mova-db`

- 执行 `0004_strm_sources.sql`。
- 所有媒体文件查询、同步、重归属和返回映射携带来源字段。
- 现有删除媒体库和删除条目的级联逻辑自动覆盖 STRM 行。

### `crates/mova-application`

- 编排 STRM 播放源解析和安全策略。
- 提供专用 HTTP 客户端、DNS/IP 校验、重定向处理和并发限制。
- 把远端失败映射为稳定业务错误码。
- 保持继续观看的版本选择语义；数据库中的保存版本已经删除时回退到同条目的其他版本。

### `apps/mova-server`

- 保留鉴权、路由和 Axum HTTP 传输职责。
- 根据 `source_kind` 分派本地文件响应或远端代理响应。
- 只转发允许的请求头和响应头。
- 使用背压流构造 `Body`，不缓存完整媒体。

### `apps/mova-web`

- 消费来源字段。
- 资源版本显示 `STRM` 标签。
- 不展示 STRM 载体大小为视频大小。
- STRM 不展示不可用的音轨切换入口和伪造的技术参数。
- 新错误码全部接入中英文案。

### `apps/mova-site` 与文档

- `docs/API.md` 定义来源字段、流接口行为和错误码。
- `docs/MEDIA_LIBRARY_SCAN.md` 定义 STRM 扫描分支。
- `docs/STRM.md` 是该能力的当前设计与安全契约。
- `docs/API.md` 的变化必须同步 `apps/mova-site` API 内容及中英文案。

## 5. 数据库迁移

`migrations/0004_strm_sources.sql` 原地增加来源字段；不得修改已经冻结的 `0001_init.sql`，也不得重建用户数据库。

迁移结构：

```sql
alter table media_files
    add column source_kind varchar(16) not null default 'local_file',
    add column stream_reference_hash varchar(64);

alter table media_files
    add constraint chk_media_files_source_kind
        check (source_kind in ('local_file', 'strm')),
    add constraint chk_media_files_stream_reference
        check (
            (source_kind = 'local_file' and stream_reference_hash is null)
            or
            (
                source_kind = 'strm'
                and stream_reference_hash is not null
                and length(stream_reference_hash) = 64
                and stream_reference_hash ~ '^[0-9a-f]{64}$'
            )
        );
```

要求：

- 既有行通过默认值升级为 `local_file`。
- 不保存 URL、最终跳转 URL、查询参数或认证信息。
- `stream_reference_hash` 为修剪 BOM 和首尾空白后、保持 URL 字节顺序的 SHA-256 小写十六进制。
- `container` 对 STRM 设为 `null`；界面通过 `source_kind` 显示 `STRM`，不要把载体扩展名当视频容器。
- STRM 技术字段、时长、码率、内嵌音轨保持空值。
- 不需要重建缓存。升级后需要重新扫描包含 `.strm` 的媒体库才能录入引用。

所有手写 `select`、`insert`、`update`、测试 seed 和 row mapper 必须同步，不能依赖仅在部分查询出现的默认字段。

## 6. STRM 文件解析规范

扫描和播放复用同一个解析器，避免两套规则漂移。

### 输入限制

- 文件最大 8 KiB；超出返回 `strm_reference_too_large`。
- 必须是 UTF-8；允许开头存在 UTF-8 BOM。
- 修剪文件首尾空白。
- 必须恰好包含一行非空内容。
- 不支持注释或第二条候选地址。
- URL 最大 4096 字节。
- 只接受 `http` 和 `https`。
- 必须包含主机。
- 允许查询参数，保证带签名地址可用。
- 禁止 URL userinfo，即 `user:password@host`。
- 禁止 fragment。

### 解析结果

```rust
pub struct HttpStrmReference {
    pub url: Url,
    pub reference_hash: String,
}
```

URL 只存在于函数返回值和当前请求内存中。`Debug` 实现不得输出完整 URL；如需要诊断，只记录 scheme、脱敏 host、端口和引用哈希前缀。

### 扫描问题

非法 STRM 不进入 `media_files`，但必须作为单文件问题写入本次扫描通知：

- `strm_reference_empty`
- `strm_reference_too_large`
- `strm_reference_invalid_utf8`
- `strm_reference_multiple_lines`
- `strm_reference_invalid_url`
- `strm_reference_unsupported_scheme`
- `strm_reference_credentials_not_allowed`

扫描最终状态为 `scan.completed_with_issues`，不是 `scan.failed`。通知最多保留现有上限数量的问题明细，且不得包含 URL。

这里只把已经成功读取后的内容格式错误视为可恢复问题。载体读取失败、目录遍历失败、
路径边界无法确认等本地 I/O 错误仍属于非权威发现结果，必须中止扫描并跳过最终删除对账，
不能把暂时不可读误判成文件已删除。

如果一个已经入库的 STRM 后来变成非法内容，该路径不属于本轮有效 `discovered_paths`，最终对账会删除旧的不可播放版本；其他版本和媒体条目按现有孤儿清理规则处理。

## 7. 扫描链路

### 7.1 发现结果

发现结果使用以下结构：

```rust
pub struct MediaDiscoveryReport {
    pub files: Vec<DiscoveredMediaFileInventory>,
    pub issues: Vec<MediaDiscoveryIssue>,
}
```

`MediaDiscoveryIssue` 至少包含载体路径、稳定原因码和受限诊断文本。路径可以进入管理员扫描通知，URL 不可以。

### 7.2 Inventory 和完整分析

`DiscoveredMediaFileInventory` 与 `DiscoveredMediaFile` 包含：

```rust
pub source_kind: MediaSourceKind,
pub stream_reference_hash: Option<String>,
```

STRM 扫描哈希在既有文件大小、修改时间和 sidecar 指纹之外加入 `source_kind` 和 `stream_reference_hash`。即使 URL 改成相同长度且保留文件时间，引用变化也会进入增量处理。本地文件继续使用既有哈希算法，避免迁移后无意义地重跑全部 `ffprobe`。

STRM 完整分析流程：

1. 解析 `.strm` 文件名和目录身份。
2. 读取 NFO、图片和外挂字幕。
3. 不调用 `ffprobe`。
4. 构造技术字段为空的 `DiscoveredMediaFile`。
5. 进入现有分组、严格 TMDB 匹配和事务写入流程。

`.strm` 和本地文件在文件名或 TMDB 身份一致时必须合并为同一媒体条目的不同 `media_files` 版本。不得因为 `source_kind` 不同创建两份电影、剧集或继续观看记录。

### 7.3 相关入口一致性

以下入口都必须识别 STRM，避免“全量扫描能识别、局部刷新不能识别”：

- 目录 inventory 扫描
- 单路径浅分析
- NFO/sidecar 刷新
- `discovered_paths` 最终对账
- 外挂字幕目录索引
- 同集多版本歧义判断
- 媒体文件重新归属和去重

不要提升 `LOCAL_ANALYSIS_VERSION`，避免让全部既有本地媒体无意义地重新运行 `ffprobe`。

## 8. HTTP/HTTPS 播放代理

### 8.1 分派

`build_media_file_stream_response` 完成用户和媒体库权限校验后：

- `local_file`：保持现有本地 Range、音轨 remux 逻辑。
- `strm`：校验载体仍在媒体库根目录内，读取并解析引用，然后进入远端代理。

当 STRM 请求带 `audio_track_id` 时返回：

```text
error_code = strm_audio_track_selection_unsupported
```

不得调用 FFmpeg。

### 8.2 专用 HTTP 客户端

在应用层构造独立客户端：

- `rustls-tls`
- 启用 reqwest `stream` feature
- 禁止自动重定向，应用层手动处理
- 禁止继承 `HTTP_PROXY`、`HTTPS_PROXY` 等环境代理
- 连接超时 8 秒
- 等待响应头超时 15 秒
- 响应体不设置固定总时长，使用每块 30 秒空闲超时
- User-Agent 为 `mova/<version>`
- 全局最多 64 条 STRM 代理流
- 每个用户最多 4 条 STRM 代理流
- 客户端断开时立即释放上游响应、信号量和 socket

并发默认值可以作为服务端常量；若提供高级环境变量，不能加入默认 Docker Compose 示例。

### 8.3 请求头

只允许向上游发送：

- `Range`
- `If-Range`
- Mova 自己的 `User-Agent`
- 必要的默认 `Accept`
- `Accept-Encoding: identity`

禁止转发：

- 客户端 `Cookie`
- 客户端 `Authorization`
- `Proxy-Authorization`
- `X-Forwarded-*`
- 任意自定义请求头

只接受单段 `bytes` Range。多段 Range、非法语法和过长头值在请求上游前拒绝。

### 8.4 响应

只透传：

- HTTP 状态 `200`、`206`、`416`
- `Content-Type`
- `Content-Length`
- `Content-Range`
- `Accept-Ranges`
- `ETag`
- `Last-Modified`

不透传 `Set-Cookie`、`Location`、`Server`、上游错误正文或其他 hop-by-hop 头。Mova 响应设置 `Cache-Control: private, no-store`。
同时设置 `X-Content-Type-Options: nosniff`，不得启用 reqwest 自动解压，避免压缩传输破坏字节 Range 语义。

允许的远端正文类型：

- `video/*`
- `audio/*`
- `application/octet-stream`
- `application/mp4`、`application/x-matroska`
- 缺失 Content-Type，由客户端尝试识别

`text/html`、JSON、XML、MPEGURL/HLS 等明显不是直接媒体文件的响应统一映射为
`remote_response_invalid`。不能将错误页面作为同源媒体内容转发。

处理规则：

- 无 Range 的 `GET`：允许上游 `200`。
- 有 Range 且上游正确返回 `206`：原样流式代理。
- `206` 必须包含可解析且与请求区间一致的 `Content-Range`；`Content-Length` 若存在，也必须与区间长度一致，否则返回 `remote_response_invalid`。
- 请求从 0 开始而上游忽略 Range 返回 `200`：允许从头播放，但不宣称支持拖动。
- 请求从非零位置开始、没有 `If-Range` 且上游返回 `200`：返回 `remote_range_not_supported`，不得下载完整文件模拟 Range。
- 请求带 `If-Range` 且验证条件失效时，上游按 HTTP 语义返回完整 `200`；服务端允许该响应，但不会宣称它满足了原 Range。
- 上游 `416`：返回规范的 Range 错误，安全透传有效 `Content-Range`。
- `HEAD`：优先上游 `HEAD`；上游返回 `405` 或 `501` 时使用 `GET Range: bytes=0-0` 探测并立即丢弃响应体。
- 上游在响应体中途失败时关闭客户端流并记录脱敏诊断，不能改写已经发送的 HTTP 状态。

使用响应字节流和 Axum `Body::from_stream`，不得 `bytes().await`、写完整临时文件或建立媒体缓存。

### 8.5 重定向

- 最多 3 次。
- 每一步解析相对 `Location` 后重新执行 URL、DNS、IP 和端口安全检查。
- 允许 HTTP 升级到 HTTPS。
- 禁止 HTTPS 降级到 HTTP。
- 不把上一跳的敏感头带到下一跳。
- 最终 URL 仍然只存在内存中。

## 9. SSRF 防护

STRM 文件即使位于管理员挂载目录，也必须按不可信输入处理。

### 9.1 默认策略

默认只允许解析到公网地址的 HTTP/HTTPS 目标。以下地址始终拒绝：

- 未指定地址
- loopback
- link-local，包括云环境 metadata 地址
- multicast
- IPv4-mapped IPv6 映射出的上述地址
- `localhost` 和 `.localhost`

RFC1918、CGNAT、ULA 等私网目标默认拒绝。

### 9.2 私网白名单

为家庭 NAS、AList 等场景提供可选高级配置：

```text
MOVA_STRM_ALLOWED_HOSTS=192.168.1.20:5244,media.home:443
```

规则：

- 仅精确 host/IP 与端口匹配。
- 不支持 `*`、后缀通配或 CIDR。
- 白名单只允许覆盖私网限制，不能覆盖 loopback、link-local、multicast 和云 metadata 禁止项。
- 未配置时不影响普通公网 HTTP/HTTPS STRM。
- 该变量写入 `docs/DEPLOYMENT.md` 的高级配置，不加入 README 的最小 Compose 示例。

### 9.3 DNS 重绑定

不能只在请求前调用一次 DNS 再让 HTTP 客户端重新解析域名。应通过专用 DNS resolver 把经过校验的地址直接交给同一次连接：

1. 解析全部 A/AAAA 地址。
2. 任一候选地址违反策略时拒绝目标。
3. HTTP 连接只能使用这批已验证地址，同时保留原始 hostname 作为 TLS SNI。
4. 每次重定向重新执行。

禁止通过环境 HTTP 代理访问 STRM，因为代理会绕过本地 DNS/IP 验证边界。STRM 代理不继承部署环境中的远端媒体代理配置。

## 10. API 契约

`MediaFileResponse` 包含来源字段：

```json
{
  "source_kind": "strm"
}
```

说明：

- `source_kind`：`local_file` 或 `strm`。
- 不包含 `stream_url`、`resolved_url` 或带查询参数的任何字段。
- STRM 的 `file_path` 是载体路径。
- STRM 的 `file_size` 是载体大小，客户端不能解释成媒体大小。

稳定错误码：

| HTTP | `error_code` | 含义 |
|---|---|---|
| 400 | `strm_audio_track_selection_unsupported` | STRM 不支持指定内嵌音轨 |
| 403 | `strm_target_forbidden` | URL、端口、DNS 或地址被安全策略拒绝 |
| 413 | `strm_reference_too_large` | 引用文件超过读取上限 |
| 416 | `remote_range_not_supported` | 上游不能满足非零 Range |
| 422 | `strm_reference_invalid` | 引用内容已经无效 |
| 429 | `strm_user_stream_limit_exceeded` | 用户并发数达到上限 |
| 502 | `remote_source_unavailable` | DNS、连接或上游失败状态 |
| 502 | `remote_response_invalid` | 上游正文类型或响应头不符合直接媒体要求 |
| 503 | `strm_stream_capacity_exhausted` | 服务端全局代理名额用尽 |
| 504 | `remote_source_timeout` | 连接或响应头超时 |

`message` 仅作诊断兜底。Web 和其他客户端根据 `error_code + params` 本地化。

应用层不得直接记录 reqwest 错误的 `Debug`/`Display` 输出，因为其中可能携带完整 URL。
应先转换为不含 URL 的内部失败类别，再记录协议、脱敏主机、端口、引用哈希前缀和错误类别。

HTTP API contract version 为 `1`；`source_kind`、来源值和错误码都属于该版本允许的加法式契约。SSE 协议不变化，也不包含 STRM 播放状态推送。

## 11. 播放进度、多版本和删除

- 播放进度仍以 `media_item_id + last_media_file_id` 保存。
- 同一电影或同一集的本地文件与 STRM 只形成多个版本，不形成多条继续观看记录。
- 保存的 STRM 载体被删除并完成扫描后，现有继续观看查询回退到同条目的其他 `media_file`。
- 回退排序应先保留用户最近使用且仍存在的版本，再选择其他版本；不能为每个版本复制进度。
- `/media-files/{id}/stream` 不得在远端失败时悄悄输出另一个 `media_file_id` 的正文，否则版本记忆和进度归属会失真。
- 远端临时不可达时返回稳定错误；客户端可以明确切换到其他版本，并用实际选中的 `media_file_id` 继续上报进度。
- STRM 没有扫描时长时，允许播放器在 loaded metadata/time update 后提交实际 duration。
- 标记已看完逻辑不变。
- 删除媒体库依赖既有数据库级联删除，不产生远端缓存清理任务，因为 STRM 不缓存远端正文。
- 一次 404、超时或断网不会删除 STRM；这类失败不是权威文件系统事实。

## 12. 外挂字幕、音轨和片头

### 支持

- 与 `.strm` 同 stem 的本地外挂字幕。
- 季集号唯一时的现有外挂字幕关联规则。
- 本地 NFO、poster、fanart、season/episode artwork。

### 不支持

- 远端媒体内嵌字幕枚举和抽取。
- 远端媒体内嵌音轨枚举和切换。
- 对 STRM 执行音轨 remux。
- 对 STRM 创建片头检测任务。

数据库的片头检测候选查询必须显式限定 `source_kind = 'local_file'`，不能依靠执行失败后重试。

## 13. Web 行为

- 文件版本卡片显示小型 `STRM` 来源标签。
- 载体路径仍可按现有规则展示，但不展示 URL。
- 文件大小字段对 STRM 显示 `—` 或“远程资源”，不展示载体文本文件大小。
- 技术规格为空时不生成空标签。
- STRM 没有内嵌音轨时隐藏音轨菜单，外挂字幕菜单继续工作。
- 播放 URL 仍由 `mediaFileStreamUrl(media_file_id)` 生成。
- HTML media element 无法读取失败响应的 JSON 时，显示统一的“远程资源暂不可用”；不要为获取详细错误额外下载媒体正文。
- 所有可见文案同时存在于中文与英文目录。

## 14. 测试要求

### 14.1 STRM 解析器

- 普通 HTTP、HTTPS
- 查询参数和签名 URL
- UTF-8 BOM、CRLF、首尾空白
- 空文件、纯空白
- 多个非空行
- 超过 8 KiB
- 非 UTF-8
- 缺少 host
- FTP、RTSP、MMS、file scheme
- URL userinfo
- fragment
- 同长度 URL 变化仍改变引用哈希和扫描哈希
- `Debug` 和错误文本不包含查询参数或完整 URL

### 14.2 扫描

- `.strm` 被 inventory 和路径对账发现
- STRM 不启动 `ffprobe`
- NFO、图片和外挂字幕正常应用
- 非法 STRM 只产生扫描问题，不失败整库
- STRM 本地读取/I/O 失败使发现结果保持非权威，不执行缺失路径删除
- 由合法变非法后删除旧版本
- URL 变化触发增量更新
- 本地视频扫描与缓存复用不回归
- 本地文件与 STRM 通过相同 TMDB ID 合并为多版本
- 扫描通知和日志不包含 URL

### 14.3 数据库

- 从 0003 状态原地迁移，旧行成为 `local_file`
- source constraint 拒绝非法组合
- insert/update/select/sync round trip
- 多版本重归属保留 `source_kind` 和哈希
- 删除媒体库级联删除 STRM 行
- 播放进度版本失效后正确回退

### 14.4 HTTP 代理

使用进程内 mock upstream，不依赖公网：

- GET 200 流式代理
- HEAD
- Range 206 和 Content-Range
- 起点为 0 时上游忽略 Range
- 非零 Range 被忽略时返回稳定错误
- 416
- 301/302/307/308，相对 Location，超过 3 次
- HTTPS 降级拒绝
- 上游 401、403、404、500 不透传正文
- 连接和响应头超时
- 响应体中途断开
- 客户端取消后释放连接和信号量
- 不转发 Cookie、Authorization、X-Forwarded-*、Set-Cookie
- 大响应保持有界内存
- 全局与用户并发限制

### 14.5 SSRF

- 直接 loopback、私网、link-local、multicast 拒绝
- IPv6 和 IPv4-mapped IPv6
- 公网主机重定向到私网拒绝
- DNS 返回混合公网/私网地址时拒绝
- 精确私网白名单通过
- 白名单不能开放 loopback 和 metadata 地址
- 环境代理不参与 STRM 请求

### 14.6 Web

- STRM 标签
- 隐藏载体大小和空技术字段
- 隐藏音轨切换、保留外挂字幕
- 版本切换和继续观看仍使用正确 `media_file_id`
- 远端失败文案中英文一致

## 15. 验证命令

至少执行：

```bash
cargo fmt --all -- --check
cargo check --workspace
cargo test -p mova-scan
cargo test -p mova-application
cargo test -p mova-server
cargo test -p mova-db -- --include-ignored
pnpm -C apps/mova-web test
pnpm -C apps/mova-web check
pnpm -C apps/mova-web build
npm --prefix apps/mova-site run check:api-docs
npm --prefix apps/mova-site run lint
npm --prefix apps/mova-site run typecheck
npm --prefix apps/mova-site run build
git diff --check
```

还必须验证：

- 新数据库从零运行全部迁移。
- 已执行到 0003 的数据库原地升级到 0004。
- Docker 内使用本地 mock HTTP 源完成播放和 Range 拖动。
- 本地视频、音轨选择、字幕和片头检测没有回归。
- UI 变更附带深色和浅色主题截图。

## 16. 一致性要求

STRM 能力必须持续满足以下条件：

- 全量与增量扫描均能发现 HTTP/HTTPS STRM。
- 扫描不访问远端、不调用 FFmpeg、不因单个非法引用失败。
- 电影、剧集、NFO、图片、外挂字幕和多版本规则与本地文件一致。
- 三端可继续使用同一个 Mova stream URL。
- Range 拖动在上游支持时正常工作。
- 所有重定向和 DNS 结果经过 SSRF 防护。
- URL、查询参数和凭据未进入数据库、API、通知或日志。
- 旧数据库可无损迁移，旧本地播放无回归。
- API、官网与中英文案同步。
- 全部自动化检查和 Docker 手工验收通过。

## 17. 参考

- [Emby STRM Files](https://support.emby.media/support/articles/Strm-Files.html)
- [FFmpeg Protocols Documentation](https://ffmpeg.org/ffmpeg-protocols.html)
