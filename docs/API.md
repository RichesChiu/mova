# Mova HTTP API

本文定义 `mova-server` HTTP 接口的用途、鉴权、请求参数、响应结构和业务语义。

## 通用说明

- Base URL：默认 `http://127.0.0.1:36080`
- HTTP API 契约版本：`1`
- 响应格式：
  - 普通业务接口默认返回 JSON，并统一包裹成 `code / message / data`
  - 错误响应额外返回稳定的 `error_code / params`，客户端必须优先使用这两个字段生成本地化文案
  - 媒体流和图片资源接口返回文件流，不返回 JSON
- 鉴权：
  - `GET /api/health`、`GET /api/auth/bootstrap-status`、`POST /api/auth/bootstrap-admin`、`POST /api/auth/login`、`POST /api/auth/token-login`、`POST /api/auth/refresh`、`POST /api/auth/logout` 可匿名访问
  - 其他接口都要求登录态
  - Web 端继续使用 session cookie
  - 原生客户端使用 `Authorization: Bearer <access_token>` 访问业务接口，`access_token` 和 `refresh_token` 通过 `POST /api/auth/token-login` 获取；`refresh_token` 只能调用 `POST /api/auth/refresh`，不能访问普通业务接口
  - 标注为管理类的接口（用户管理、建库、删库、触发扫描、服务器根目录等）允许 `owner` 和 `admin`；只有用户角色提升等所有者操作明确要求 `owner`
  - `GET /api/realtime/events` 返回 `text/event-stream`，不使用统一 JSON envelope
- 成功格式：

```json
{
  "code": 200,
  "message": "ok",
  "data": {
    "...": "..."
  }
}
```

- 错误格式：

```json
{
  "code": 404,
  "error_code": "resource_not_found",
  "params": {},
  "message": "media item not found: 42",
  "data": null
}
```

其中：

- `code` 始终是数字 HTTP 状态码；业务分类由 `error_code` 表达。
- `error_code` 是跨 Web、macOS 和 iOS 稳定的机器可读原因码。
- `params` 是本地化模板参数；没有参数时仍返回空对象。
- `message` 是英文诊断信息，只用于日志、排障和未知 `error_code` 的兜底，不应直接作为客户端主文案。

通用错误码包括 `invalid_request`、`resource_conflict`、`unauthorized`、`forbidden`、`resource_not_found`、`rate_limited`、`service_unavailable`、`range_not_satisfiable` 和 `internal_error`。Token 相关错误码包括 `token_expired`、`invalid_token`、`invalid_refresh_token`、`refresh_token_expired` 和 `session_revoked`。`rate_limited.params.retry_after_seconds` 与响应头 `Retry-After` 保持一致；`range_not_satisfiable.params.file_size` 表示文件总字节数。

账户与用户管理接口还会返回以下稳定业务错误码：

| `error_code` | `params` | 含义 |
| --- | --- | --- |
| `bootstrap_unavailable` | `{}` | 首个管理员初始化已经完成 |
| `authentication_required` | `{}` | 当前请求需要登录 |
| `invalid_credentials` | `{}` | 账户或密码错误 |
| `account_disabled` | `{ "account": string }` | 账户已停用 |
| `account_already_exists` | `{}` | 账户名称已存在 |
| `user_not_found` | `{ "user_id": number }` | 用户不存在 |
| `field_required` | `{ "field": string }` | 必填字段为空 |
| `field_too_long` | `{ "field": string, "max": number }` | 字段超过长度上限 |
| `field_too_short` | `{ "field": string, "min": number }` | 字段未达到长度下限 |
| `invalid_role` | `{ "allowed": string[] }` | 用户角色不受支持 |
| `password_unchanged` | `{}` | 新旧密码相同 |
| `invalid_current_password` | `{}` | 当前密码错误 |
| `self_management_not_allowed` | `{ "operation": string }` | 用户管理接口不允许对自己的账户执行该操作 |
| `insufficient_privilege` | `{ "actor_role": string, "target_role": string }` | 不能管理同级或更高权限账户 |
| `admin_required` | `{}` | 操作需要管理员权限 |
| `owner_required` | `{ "operation": string }` | 操作需要系统管理员权限 |
| `owner_role_not_assignable` | `{}` | 系统管理员角色不能通过用户管理接口分配 |
| `last_admin_required` | `{}` | 必须保留至少一个已启用的管理员 |

媒体处理接口还会返回以下稳定业务错误码：

| `error_code` | `params` | 含义 |
| --- | --- | --- |
| `subtitle_too_large` | `{ "max_bytes": number }` | 字幕源文件或转换后的 WebVTT 超过服务端单次处理上限 |
| `strm_audio_track_selection_unsupported` | `{}` | STRM 不支持指定内嵌音轨 |
| `strm_reference_too_large` | `{}` | STRM 引用载体超过读取上限 |
| `strm_reference_invalid` | `{}` | STRM 引用内容无效 |
| `strm_target_forbidden` | `{}` | STRM 目标被 URL、端口、DNS 或地址安全策略拒绝 |
| `remote_range_not_supported` | `{}` | STRM 上游不能满足非零 Range |
| `strm_user_stream_limit_exceeded` | `{}` | 当前用户的 STRM 并发数达到上限 |
| `remote_source_unavailable` | `{}` | STRM 上游不可用或返回失败状态 |
| `remote_response_invalid` | `{}` | STRM 上游内容类型或 Range 响应不符合直接媒体要求 |
| `remote_source_timeout` | `{}` | STRM 上游连接或响应头超时 |
| `strm_stream_capacity_exhausted` | `{}` | 服务端 STRM 全局代理名额已满 |

客户端必须允许服务端增加新的 `error_code`。已知错误码使用本地文案；未知错误码可以临时显示 `message`，并应将其记录为诊断信息。

- 文档中的字段示例多数只展示 `data` 内部结构，实际响应会额外包一层统一 envelope。

- 常见状态码：
  - `200 OK`：请求成功
  - `201 Created`：创建成功
  - `202 Accepted`：异步任务已创建并开始后台执行
  - `400 Bad Request`：请求参数或业务校验不通过
  - `401 Unauthorized`：未登录、access token 无效/过期，或 refresh token 无效/过期/已撤销
  - `403 Forbidden`：已登录但没有权限访问，或请求目标被服务端安全策略拒绝
  - `404 Not Found`：资源不存在
  - `409 Conflict`：资源当前状态不允许执行该操作
  - `413 Payload Too Large`：请求关联的媒体处理输入或生成结果超过服务端上限
  - `416 Range Not Satisfiable`：本地媒体 `Range` 越界，或 STRM 上游不能满足非零 Range
  - `422 Unprocessable Entity`：请求结构有效，但 STRM 引用内容无法作为受支持的远程媒体地址处理
  - `429 Too Many Requests`：认证失败次数或当前用户的远程流并发数达到限制；提供 `Retry-After` 时按该响应头等待后重试
  - `500 Internal Server Error`：服务内部错误
  - `502 Bad Gateway`：STRM 上游不可用，或返回的直接媒体响应无效
  - `503 Service Unavailable`：媒体处理资源暂时繁忙、STRM 全局代理名额已满、磁盘安全余量不足或依赖服务暂不可用，客户端可以稍后重试
  - `504 Gateway Timeout`：STRM 上游连接或响应头超时
- TMDB provider 从运行时环境变量 `MOVA_TMDB_ACCESS_TOKEN` 读取，值必须是 TMDB 账户 API 设置页中的 **API Read Access Token**，不是较短的 `API Key (v3 auth)`。变量为空或只含空白时服务仍正常启动，本地扫描、NFO/sidecar、入库和播放保持可用；扫描不会发起 TMDB 请求，条目以 `skipped / metadata_provider_disabled` 完成。后续配置 Token、重启并重扫后，这些条目会进入远端补全。每个媒体库可单独配置 `metadata_language`，决定扫描与元数据补全时使用 `zh-CN` 或 `en-US`。TMDB 接入、身份匹配规则和字段覆盖见 [`TMDB_INTEGRATION.md`](TMDB_INTEGRATION.md)，完整 v3 接口目录见 [`TMDB.md`](TMDB.md)。
- TMDB 详情响应中的 `vote_average` 和 `vote_count` 会写入通用 `ratings` 集合，评分来源明确标记为 `tmdb`。TMDB details 附带的 IMDb、TVDB、Wikidata 和社交平台 ID 只作为外部身份保存，不代表对应平台的评分或数据已经接入；当前不请求 IMDb、OMDb 或其他评分来源。
- 本地海报和背景图的 URL 带版本参数（例如 `/api/media-items/42/poster?v=1704164645`）。浏览器可以长期缓存；媒体元数据更新时版本参数随之变化。

## 接口总览

| Method | Path | 作用 |
| --- | --- | --- |
| `GET` | `/api/health` | 健康检查 |
| `GET` | `/api/auth/bootstrap-status` | 查询是否需要初始化系统所有者 |
| `POST` | `/api/auth/bootstrap-admin` | 初始化系统所有者并登录 |
| `POST` | `/api/auth/login` | 登录 |
| `POST` | `/api/auth/token-login` | 为原生客户端创建 access token 和 refresh token |
| `POST` | `/api/auth/refresh` | 使用 refresh token 轮换并获取新的 token |
| `POST` | `/api/auth/logout` | 登出 |
| `GET` | `/api/auth/me` | 查询当前用户 |
| `PATCH` | `/api/auth/me` | 更新当前用户昵称 |
| `GET` | `/api/home` | 查询当前用户的轻量首页快照 |
| `GET` | `/api/realtime/state` | 查询当前可见资源版本和活跃扫描 |
| `GET` | `/api/realtime/events` | 订阅资源失效与临时扫描进度（SSE） |
| `PUT` | `/api/auth/password` | 当前用户修改自己的密码 |
| `GET` | `/api/users` | 查询用户列表（管理员） |
| `POST` | `/api/users` | 创建用户（管理员） |
| `PATCH` | `/api/users/{id}` | 更新低权限用户的角色、启用状态与媒体库权限（管理员） |
| `DELETE` | `/api/users/{id}` | 删除用户（管理员） |
| `PUT` | `/api/users/{id}/password` | 管理员重置指定用户密码 |
| `GET` | `/api/notifications` | 查询当前用户可见的通知，可选仅返回未读项 |
| `PUT` | `/api/notifications` | 批量标记当前用户的通知为已读 |
| `PUT` | `/api/notifications/{id}/read` | 标记一条可见通知为已读 |
| `GET` | `/api/server/media-tree` | 查询服务端当前可用于建库的媒体文件夹树（管理员） |
| `GET` | `/api/libraries` | 查询媒体库列表 |
| `GET` | `/api/libraries/recently-added` | 查询按库分组的最新添加内容 |
| `POST` | `/api/libraries` | 创建媒体库（管理员） |
| `GET` | `/api/libraries/{id}` | 查询单个媒体库详情 |
| `PATCH` | `/api/libraries/{id}` | 更新媒体库基础配置（管理员） |
| `DELETE` | `/api/libraries/{id}` | 删除媒体库（管理员） |
| `GET` | `/api/libraries/{id}/media-items` | 查询媒体库下的媒体条目列表 |
| `GET` | `/api/libraries/{id}/scan-jobs` | 查询媒体库扫描历史（管理员） |
| `GET` | `/api/libraries/{id}/scan-jobs/{scan_job_id}` | 查询单个扫描任务状态（管理员） |
| `POST` | `/api/libraries/{id}/scan` | 触发异步扫描（管理员） |
| `GET` | `/api/search` | 搜索当前用户可见库下的电影、剧集和集条目 |
| `GET` | `/api/media-items/{id}` | 查询单个媒体条目详情 |
| `GET` | `/api/media-items/{id}/metadata-sources` | 查询条目的元数据来源摘要（管理员） |
| `GET` | `/api/media-items/{id}/metadata-sources/{source_id}` | 查询并观察单个本地元数据来源（管理员） |
| `GET` | `/api/media-items/{id}/cast` | 查询单个媒体条目的演员列表 |
| `GET` | `/api/media-items/{id}/playback-header` | 查询播放器页头部信息 |
| `GET` | `/api/media-items/{id}/files` | 查询媒体条目关联文件列表 |
| `GET` | `/api/media-items/{id}/episode-outline` | 查询剧集全集大纲并标记本地可用集 |
| `GET` | `/api/media-items/{id}/metadata-search` | 手动搜索单条媒体的候选元数据（管理员） |
| `POST` | `/api/media-items/{id}/metadata-match` | 选择候选结果并替换当前媒体元数据（管理员） |
| `POST` | `/api/media-items/{id}/refresh-metadata` | 手动重拉单个媒体条目元数据（管理员） |
| `GET` | `/api/media-items/{id}/poster` | 读取媒体条目海报图 |
| `GET` | `/api/media-items/{id}/backdrop` | 读取媒体条目背景图 |
| `GET` | `/api/media-items/{id}/logo` | 读取媒体条目透明标题 Logo |
| `GET` | `/api/seasons/{id}/poster` | 读取某一季海报图 |
| `GET` | `/api/seasons/{id}/backdrop` | 读取某一季背景图 |
| `GET` | `/api/media-items/{id}/playback-progress` | 查询单条内容的最近播放进度 |
| `PUT` | `/api/media-items/{id}/playback-progress` | 写入或更新播放进度 |
| `GET` | `/api/playback-progress/continue-watching` | 查询继续观看列表 |
| `GET` | `/api/media-files/{id}/audio-tracks` | 查询媒体文件可切换的内嵌音轨列表 |
| `GET` | `/api/media-files/{id}/subtitles` | 查询媒体文件可切换字幕列表 |
| `GET` | `/api/media-files/{id}/stream` | 播放媒体文件 |
| `HEAD` | `/api/media-files/{id}/stream` | 只读查询播放头信息，不生成音轨缓存 |
| `GET` | `/api/subtitle-files/{id}/stream` | 输出单条字幕轨道的 WebVTT 内容 |
| `HEAD` | `/api/subtitle-files/{id}/stream` | 只读查询字幕 WebVTT 头信息，不生成字幕缓存 |

## 1. 健康检查

### `GET /api/health`

作用：
- 检查服务进程和数据库是否可用

典型场景：
- 本地调试
- 容器探针
- 部署后联通性检查

返回：
- 成功时返回 `200 OK`

```json
{
  "status": "ok",
  "version": "development",
  "api_version": 1
}
```

`version` 是当前运行构建的权威版本标识。官方镜像在构建阶段使用不可变镜像 tag 注入该值；源码构建默认回退到 Cargo package version。自定义镜像可以通过 Docker build argument `MOVA_BUILD_VERSION` 注入版本，运行容器时不能改写已经编译进二进制的值。

`api_version` 是 HTTP 契约版本。版本 `1` 内允许新增可选字段、endpoint 和 `error_code`；删除或改变既有字段语义、鉴权规则、状态码或 endpoint 属于破坏性变更，必须提升契约版本。SSE 使用独立的 `protocol_version`，规则见 [`SSE.md`](SSE.md)。

## 2. 认证与用户

初始化、登录和创建用户接口使用 `username` 作为登录账户字段，界面将它展示为账户。服务端会去除首尾空白，并限制为 1–254 个字符，因此可以使用普通账号名或邮箱形式的登录标识；邮箱形式只作为账户字符串，不代表 Mova 会校验邮箱归属或发送邮件。账户按去除空白后的 Unicode 小写值唯一并用于登录查找，因此大小写不同不能创建为两个账户。账户创建后不可修改，昵称初始化为账户名称，之后只能由用户本人通过个人设置修改。

Web session、原生 access token 和 refresh token 在数据库中都只保存 SHA-256 hash，原始凭据只在签发时返回给客户端。

密码认证默认按账户合并 Web 与原生客户端的失败次数：5 分钟窗口内达到 5 次失败后锁定 15 分钟。当前用户修改密码使用独立限制，只把当前密码校验失败计入次数。受限请求返回 `429 Too Many Requests`，并通过 `Retry-After` 返回剩余等待秒数。服务端只在内存中保留最多 4096 个近期键，成功认证会清除对应失败状态；服务重启会清空该临时状态。部署方可以通过 `MOVA_AUTH_RATE_LIMIT_MAX_FAILURES`、`MOVA_AUTH_RATE_LIMIT_WINDOW_SECONDS`、`MOVA_AUTH_RATE_LIMIT_LOCKOUT_SECONDS` 和 `MOVA_AUTH_RATE_LIMIT_MAX_KEYS` 调整正整数配置。

### `GET /api/auth/bootstrap-status`

作用：
- 查询当前系统是否还没有系统所有者，前端可据此决定显示“初始化首个账户”还是普通登录页

返回：
- `200 OK`

```json
{
  "bootstrap_required": true
}
```

### `POST /api/auth/bootstrap-admin`

作用：
- 仅在系统还没有管理账户时，创建唯一的 `owner` 用户并直接建立登录态

请求体：

```json
{
  "username": "admin",
  "password": "admin123456"
}
```

说明：
- 一旦系统里已经存在 `owner` 或 `admin`，再调用会返回 `409 Conflict`
- 成功后会写入 session cookie

### `POST /api/auth/login`

作用：
- 使用用户名和密码登录

请求体：

```json
{
  "username": "admin",
  "password": "admin123456"
}
```

说明：
- 当前登录账户精确匹配
- 密码最少 8 位
- 成功后会写入 session cookie
- 达到认证失败限制时返回 `429 Too Many Requests` 和 `Retry-After`

### `POST /api/auth/token-login`

作用：
- 使用用户名和密码登录，并返回原生客户端使用的短期 `access_token` 和长期 `refresh_token`

请求体：

```json
{
  "username": "admin",
  "password": "admin123456",
  "device_name": "Mova iOS",
  "client_type": "native-ios"
}
```

字段说明：
- `device_name`：可选，客户端设备名称，用于服务端追踪设备会话
- `client_type`：可选，客户端类型；默认 `native`

返回：

```json
{
  "access_token": "short-lived-access-token",
  "access_token_type": "Bearer",
  "access_token_expires_at": "2026-06-25T10:30:00Z",
  "refresh_token": "long-lived-refresh-token",
  "refresh_token_expires_at": "2026-07-25T10:00:00Z",
  "user": {
    "id": 1,
    "username": "admin",
    "nickname": "admin",
    "role": "owner",
    "is_enabled": true,
    "library_ids": []
  }
}
```

说明：
- `access_token` 默认有效期 2 小时，只用于访问普通业务接口
- `refresh_token` 默认有效期 30 天，只用于调用 `POST /api/auth/refresh`
- 服务端只保存 token hash，不明文保存原始 token
- 业务请求通过 `Authorization: Bearer <access_token>` 访问受保护接口
- access token 过期、refresh token 过期/撤销、用户被禁用/删除/改密后，对应原生客户端会话会失效
- Web 端使用 `POST /api/auth/login`，不调用原生客户端登录接口

### `POST /api/auth/refresh`

作用：
- 使用有效 `refresh_token` 轮换当前原生客户端设备会话，并返回新的 `access_token` 和 `refresh_token`

请求体：

```json
{
  "refresh_token": "long-lived-refresh-token"
}
```

返回：

```json
{
  "access_token": "new-short-lived-access-token",
  "access_token_type": "Bearer",
  "access_token_expires_at": "2026-06-25T12:30:00Z",
  "refresh_token": "new-long-lived-refresh-token",
  "refresh_token_expires_at": "2026-07-25T12:00:00Z",
  "user": {
    "id": 1,
    "username": "admin",
    "nickname": "admin",
    "role": "owner",
    "is_enabled": true,
    "library_ids": []
  }
}
```

说明：
- refresh 成功后旧 `refresh_token` 会立即失效
- 旧 `refresh_token` 在其原始有效期内被重复使用时，服务端会视为异常重放，并在同一数据库事务内撤销对应原生客户端设备会话
- 并发提交同一个 `refresh_token` 时只允许一次轮换成功；其余请求会按重放处理并撤销该设备会话。原生客户端必须对同一设备会话串行执行 refresh，不能并发重试
- 已超过原始有效期的历史 `refresh_token` 不会撤销当前设备会话；历史记录仍保留时返回 `refresh_token_expired`，清理后返回 `invalid_refresh_token`
- 用户被禁用、删除或改密后，旧 `access_token` 和 `refresh_token` 都不能继续使用
- 失败时常见 `error_code` 包括 `invalid_refresh_token`、`refresh_token_expired`、`session_revoked`

### `POST /api/auth/logout`

作用：
- 删除当前登录态对应的服务端会话记录；如果当前是 cookie 登录，还会顺带清理 session cookie

可选请求体：

```json
{
  "refresh_token": "long-lived-refresh-token"
}
```

Web cookie 会话退出时可以完全省略请求体，也不需要发送 `Content-Type: application/json`。如果发送请求体，则必须是合法 JSON。

返回：
- `200 OK`

说明：
- 支持 cookie、Bearer access token 和请求体里的 `refresh_token`
- 接口保持幂等并允许匿名调用；没有有效登录凭据时仍会清除 Web session cookie 并返回成功
- 如果同时带了 cookie 和 `Authorization`，服务端会优先使用 Bearer token
- 原生客户端应尽量在登出时同时提交当前 `refresh_token`；如果 access token 已过期但 refresh token 仍有效，服务端仍会撤销对应设备会话
- 会话撤销在事务提交后会向使用该凭据建立的 SSE 连接发送 `session.invalidated`，`reason = session_revoked`，随后关闭连接；同一用户的其他设备会话不受影响

### `GET /api/auth/me`

作用：
- 查询当前登录用户

返回：
- `200 OK`
- 返回字段包括 `id`、`username`、`nickname`、`role`、`is_enabled`、`library_ids`
- 支持 cookie 和 Bearer access token 两种登录态；不接受 refresh token
- `role` 使用 `owner` / `admin` / `viewer`。初始化接口创建的唯一 `owner` 是系统所有者；`owner` 和 `admin` 都拥有全部媒体库访问权

### `PATCH /api/auth/me`

作用：
- 更新当前登录用户的昵称

请求体：

```json
{
  "nickname": "Cinema Fan"
}
```

说明：
- 昵称留空时，服务端会自动回退为用户名
- 这是修改昵称的唯一接口，管理员用户管理接口不能修改其他用户的昵称
- 成功后会直接返回更新后的当前用户对象
- 支持 cookie 和 Bearer token 两种登录态

### `GET /api/home`

作用：
- 一次返回当前用户首页需要的有界快照，避免 Web、macOS 和 iOS 为首页逐库分页拉取完整媒体目录。

返回：
- `current_user`：当前用户。
- `libraries`：当前用户可见媒体库的详情摘要，每个库的 `preview_items` 最多 16 条。
- `recently_added`：按库分组的最新添加内容，每个库最多 8 条。
- `continue_watching`：当前用户未看完的继续观看队列，最多 20 条。
- `realtime`：本次快照对应的 `server_epoch` 和当前可见资源 `resources` revisions。
  - `protocol_version`：SSE 同步协议版本，固定为 `1`。

说明：
- 进入具体媒体库后再使用 `GET /api/libraries/{id}/media-items` 分页加载完整目录。
- 客户端可以把 `realtime.resources` 作为当前**首页读模型**的 revision 基线，避免紧接着收到重复失效通知后再次刷新首页；它不能替代媒体详情、用户管理等独立读模型的首次加载或失效处理。

### SSE 同步协议

资源 revision、SSE 事件触发条件、完整 payload、跨 Web/macOS/iOS 客户端状态机和断线恢复见 [`SSE.md`](SSE.md)。本节保留接口级摘要。

### `GET /api/realtime/state`

作用：
- 返回当前客户端有权看到的持久化资源版本和活跃扫描，用于首次登录、SSE 重连、App 回到前台或收到 `resync.required` 后恢复状态。

典型返回：

```json
{
  "protocol_version": 1,
  "server_epoch": "019f...",
  "resources": {
    "admin:libraries": 14,
    "library:7:settings": 3,
    "library:7:catalog": 128,
    "library:7:scan": 9,
    "user:12:continue-watching": 39,
    "user:12:profile": 2
  },
  "active_scans": []
}
```

说明：
- `server_epoch` 在同一数据库生命周期内保持稳定；数据库重建后会变化。客户端发现 epoch 变化时应丢弃本地 revision 基线并重新同步。
- `resources` 只包含当前用户有权访问的资源；尚未变化过的资源 revision 为 `0`。
- `active_scans` 返回当前仍为 `pending` 或 `running` 的扫描任务。扫描 `phase` 和任务级 `progress_percent` 都会持久化，不依赖 SSE 临时状态恢复。

### `GET /api/realtime/events`

作用：
- 订阅资源失效通知与临时扫描进度。SSE 不承载最终业务数据，也不保证客户端收到每一条临时进度。

说明：
- 需要登录态，支持 cookie 和 Bearer access token。
- 返回类型为 `text/event-stream`，服务端每 15 秒发送 keep-alive。
- 服务端只推送连接建立之后的新事件，不回放历史；客户端重连后必须先调用 `GET /api/realtime/state` 做 revision 差异同步。
- 事件类型为 `resources.changed`、`scan.progress`、`scan.finished`、`resync.required` 和 `session.invalidated`。
- 可见资源键包括 `admin:libraries`、`admin:users`、`admin:notifications`、`library:{id}:settings`、`library:{id}:catalog`、`library:{id}:scan`、`library:{id}:notifications`、`user:{id}:libraries`、`user:{id}:profile`、`user:{id}:continue-watching` 和 `user:{id}:notifications`。
- `resources.changed` 只表示指定读模型需要重新读取；`scan.progress` 只承载可丢失的临时展示状态。
- `scan_job.progress_percent` 是服务端持久化的权威进度，客户端不得自行估算。
- `resync.required` 与 `session.invalidated` 发送后会关闭连接。
- SSE 连接与建立连接时使用的 session cookie 或 Bearer access token 有效期绑定。凭据到期时服务端发送 `session.invalidated`，payload 的 `reason` 为 `credential_expired`，随后关闭连接。
- 当前 Web session 被删除，或当前原生设备会话被登出、refresh token 重放保护或改密流程撤销时，服务端发送 `session.invalidated`，payload 的 `reason` 为 `session_revoked`，随后关闭连接。该信号只发送给对应凭据。
- 收到 `credential_expired` 后不能使用同一凭据直接重连：Web 端需要重新建立登录态；原生客户端需要先轮换 access token。获得有效凭据后，客户端先调用 `GET /api/realtime/state` 对账，再重新建立 SSE 连接。
- 收到 `session_revoked` 后必须清除当前凭据并重新登录，不能继续使用同一 session cookie、access token 或 refresh token 重连。

事件 payload、资源键触发条件、合并窗口、终态屏障、权限 scope 和客户端恢复算法统一见 [`SSE.md`](SSE.md)。

### `PUT /api/auth/password`

作用：
- 当前登录用户修改自己的密码

请求体：

```json
{
  "current_password": "old-password",
  "new_password": "new-password-123"
}
```

说明：
- 支持 cookie 和 Bearer token 两种登录态
- `current_password` 必须正确
- `new_password` 最少 8 位
- `new_password` 不能和当前密码相同
- 密码更新、旧 Web session 删除、现有原生客户端 access/refresh token 撤销和新 Web session 创建在同一个数据库事务中提交；任一步失败都会整体回滚
- 修改成功后响应会写回新的 session cookie；原生客户端应使用新密码重新调用 `POST /api/auth/token-login`

### `GET /api/users`

作用：
- 管理员查看当前所有用户

权限：
- `owner` 和 `admin` 可用

说明：
- `owner` / `admin` 用户的 `library_ids` 始终为空数组，语义上表示“默认拥有全部媒体库访问权”
- `viewer` 用户的 `library_ids` 表示允许访问的媒体库 ID 列表
- `owner` 是唯一系统所有者，可以管理 `admin` 和 `viewer`；`admin` 可以管理 `viewer`，不能管理平级管理员或所有者

### `POST /api/users`

作用：
- 管理员创建一个新用户

权限：
- `owner` 和 `admin` 可用；可创建的角色仍受下方权限层级约束

请求体：

```json
{
  "username": "viewer01",
  "password": "viewer1234",
  "role": "viewer",
  "is_enabled": true,
  "library_ids": [1, 2]
}
```

字段说明：
- `username`：用于登录的账户；服务端会去除首尾空白，长度必须为 1–254 个字符，可使用普通账号名或邮箱形式的精确匹配字符串
- 新用户的 `nickname` 会初始化为规范化后的 `username`；请求不能指定昵称，用户登录后只能通过 `PATCH /api/auth/me` 修改自己的昵称
- `role`：用户管理接口只支持创建 `admin` / `viewer`；`owner` 只能由系统初始化接口创建
- `library_ids`：只对 `viewer` 生效；`owner` / `admin` 会忽略这个字段

权限约束：
- 只有 `owner` 可以创建新的 `admin`
- 普通管理员只能创建 `viewer`

### `PATCH /api/users/{id}`

作用：
- 管理员更新低权限用户的角色、启用状态和媒体库访问范围

权限：
- `owner` 和 `admin` 可用；只能管理权限层级严格低于自己的用户

请求体：

```json
{
  "role": "viewer",
  "is_enabled": true,
  "library_ids": [1, 2]
}
```

字段说明：
- 所有字段都可选，不传表示保持原值
- `username` 和 `nickname` 都不属于该接口；账户不可修改，昵称只能由用户本人通过 `PATCH /api/auth/me` 修改，提交这些字段会返回请求校验错误
- `library_ids` 是更新 `viewer` 媒体库访问范围的唯一字段；传入数组会整体替换原授权，不传则保持原值
- `library_ids` 只对 `viewer` 生效；更新为 `admin` 时会自动清空库授权

关键约束：
- 权限层级固定为 `owner > admin > viewer`，调用者只能管理权限层级严格低于自己的用户
- 当前用户不能通过该接口修改自己
- 不能降级、禁用最后一个启用中的管理员
- 禁用状态、媒体库权限和会话撤销在同一个数据库事务中提交；禁用用户后会清理其现有 Web session，并撤销原生客户端 access/refresh token 会话
- 只有 `owner` 可以编辑、启用或禁用普通管理员
- 普通管理员不能修改或降级其他管理员，也不能修改 `owner`

### `DELETE /api/users/{id}`

作用：
- 管理员删除指定用户

权限：
- `owner` 和 `admin` 可用；只能删除权限层级严格低于自己的用户

说明：
- 当前用户不能删除自己
- 不能删除最后一个启用中的管理员
- 删除后会级联清理该用户的库授权、会话和播放进度
- 只有 `owner` 可以删除普通管理员
- `owner` 本身不能通过该接口被删除

返回：
- `200 OK`
- 返回统一 envelope，`message` 为 `user deleted`，`data` 为 `null`

### `PUT /api/users/{id}/password`

作用：
- 管理员重置指定用户密码

请求体：

```json
{
  "new_password": "viewer-reset-123"
}
```

说明：
- `new_password` 最少 8 位
- 当前用户不能通过该接口重置自己的密码；应使用 `PUT /api/auth/password`
- 密码更新、现有 Web session 删除和原生客户端 access/refresh token 撤销在同一个数据库事务中提交；任一步失败都会整体回滚
- 重置成功后，该用户现有 Web session 和原生客户端 access/refresh token 会话会全部失效
- 只有 `owner` 可以重置普通管理员密码

## 3. 通知中心

通知中心使用稳定外壳承载不同业务来源的消息。通知对象不等同于 SSE 事件：通知和已读状态持久化在 PostgreSQL，SSE 只通过 `*:notifications` revision 提醒客户端重新读取本节接口，不携带通知正文，也不作为通知结果的权威来源。

标准类别：

| `category` | 用途 |
| --- | --- |
| `scan` | 扫描完成、扫描失败和扫描质量问题 |
| `system` | 服务级运行状态、升级和维护消息 |
| `library` | 不属于具体扫描任务的媒体库变更 |
| `account` | 当前用户账户、安全和权限相关消息 |

类别允许继续扩展。客户端遇到未知类别时应放入“全部”列表并使用通用样式，不得丢弃。`notification_type` 使用 `<category>.<action>` 命名；扫描任务终态对应 `scan.completed`、`scan.completed_with_issues`、`scan.failed` 和 `scan.cancelled`，这四种类型是扫描结果的权威状态。客户端不得根据 `severity`、`payload.status`、问题数量或统计字段反推扫描结果。未知类型至少应展示通用通知占位和创建时间。

通知级别固定为 `info`、`success`、`warning`、`error`。可见范围由服务端在写入时确定为 server、admin、library 或 user，客户端不传 audience，也不能读取自己权限外的通知。

各类通知共用以下信息骨架，具体业务类型可以在骨架下增加自己的详情：

| 信息 | 权威来源 | 展示规则 |
| --- | --- | --- |
| 类型 | `category` / `notification_type` | 标识扫描、系统、媒体库或账户等消息来源 |
| 结果 | `notification_type` | 表达成功、有问题地完成、失败、取消或其他业务结果 |
| 对象 | `library_id`、`payload.library_name` 等类型相关字段 | 标识本次通知涉及的媒体库、账户或系统对象 |
| 原因 | `reason_code` / `reason_params` | 由客户端本地化为用户可理解的主说明 |
| 诊断 | `diagnostic_message` 等诊断字段 | 作为次级排障信息展示，不代替本地化主说明 |

### `GET /api/notifications`

查询参数：

- `category`：可选，按单个通知类别过滤；仅允许 ASCII 字母、数字、`-`、`_`，最长 32 个字符。
- `limit`：可选，默认 `20`，范围 `1–50`。
- `unread_only`：可选布尔值，默认 `false`；为 `true` 时 `items` 只返回当前用户尚未读取的通知。

返回 `NotificationFeedResponse`：

```json
{
  "items": [
    {
      "id": 92,
      "category": "scan",
      "notification_type": "scan.completed_with_issues",
      "severity": "warning",
      "library_id": 7,
      "payload": {
        "scan_job_id": 41,
        "library_id": 7,
        "library_name": "Movies",
        "status": "success",
        "summary_available": true,
        "total_files": 50,
        "reused_files": 0,
        "matched_files": 49,
        "unmatched_files": 0,
        "failed_files": 1,
        "skipped_files": 0,
        "probe_warning_count": 1,
        "issue_count": 1,
        "reason_code": null,
        "reason_params": {},
        "diagnostic_message": null,
        "issues": [
          {
            "item_key": "movie:a-minecraft-movie:2025",
            "media_type": "movie",
            "title": "A Minecraft Movie",
            "year": 2025,
            "file_count": 1,
            "metadata_status": "failed",
            "reason_code": "metadata_provider_error",
            "reason_params": {},
            "diagnostic_message": "operation timed out",
            "probe_warning_count": 1,
            "probe_warning_file_path": "/media/movies/A Minecraft Movie/A.Minecraft.Movie.2025.mkv",
            "probe_warning_code": "media_probe_warning",
            "probe_warning_params": {
              "count": 1
            },
            "probe_warning_diagnostic": "ffprobe failed: EBML header parsing failed"
          }
        ]
      },
      "is_read": false,
      "read_at": null,
      "created_at": "2026-07-16T10:06:20+08:00"
    }
  ],
  "total_unread": 3,
  "unread_by_category": {
    "scan": 2,
    "system": 1
  }
}
```

语义：

- `items` 按 `created_at desc, id desc` 排序，并应用 `category`、`unread_only` 与 `limit`；`unread_only=true` 时已读通知不会出现在列表中。
- `total_unread` 和 `unread_by_category` 始终统计当前用户可见的全部未读通知，不受本次 `category` 筛选影响，因此客户端只需一次响应即可渲染总红点和分类角标。
- `is_read` / `read_at` 是当前登录用户自己的状态；同一条 server、admin 或 library 通知可以被不同用户独立阅读。
- Web 通知中心使用 `unread_only=true`，单条或批量标记已读成功后立即重新请求当前通知列表，使已读项从弹层中消失。
- `payload` 是按 `notification_type` 区分的扩展对象。扫描通知包含 `summary_available` 布尔值、任务级计数字段，并最多内嵌 20 个未匹配、provider 失败或本地探测警告的问题摘要；`issue_count` 可能大于 `issues.length`。
- 只有 `summary_available = true` 时，`total_files`、`reused_files`、`matched_files`、`unmatched_files`、`failed_files`、`skipped_files`、`probe_warning_count` 和 `issue_count` 才是可用于展示的任务终态摘要。`scan.failed` 和 `scan.cancelled` 通常返回 `false`；此时计数字段只用于保持 payload 结构稳定，客户端不得把默认值 `0` 当作真实统计，也不应渲染成功摘要。
- `scan.completed`、`scan.completed_with_issues`、`scan.failed` 和 `scan.cancelled` 分别表示完整成功、有问题地完成、执行失败和主动取消。客户端以 `notification_type` 决定结果文案与视觉状态。
- `scan.cancelled` 使用 `info` 级别，payload 的 `status` 为 `cancelled`，表示任务被主动终止；它不等同于扫描成功或执行失败。
- 已有远端 binding 的条目在 provider 临时故障时仍可保持 `metadata_status=matched`；此时问题摘要使用 `reason_code=metadata_provider_error` 表示本次刷新失败，客户端不得把它解释为身份匹配失效。
- 扫描任务和单项问题使用 `reason_code / reason_params` 生成本地化主文案。常见原因码包括 `scan_execution_failed`、`metadata_provider_error`、`no_remote_match`、`metadata_provider_disabled`、`metadata_processing_failed` 和 `media_probe_warning`。
- `diagnostic_message` 与 `probe_warning_diagnostic` 仅供日志和排障使用。客户端不得把这些英文诊断信息直接作为通知主文案；未知原因码才允许将其作为次级兜底。
- 扫描摘要由 worker 在远端组成功提交后累计，并在任务终态直接写入通知；服务端不提供第二套扫描报告接口。更底层的网络、provider 与 `ffprobe` 排障信息由运维侧查看服务日志。
- `cache.cleanup.failed` 是仅管理员可见的 `system / error` 通知。它表示媒体库权威数据已经删除，但 `MOVA_CACHE_DIR/libraries/{library_id}` 在 10 次尝试后仍无法移除；payload 包含 `background_job_id`、`library_id`、删除前的 `library_name`、`attempt_count`、`max_attempts`、`reason_code=cache_cleanup_failed`、`reason_params` 和可选 `diagnostic_message`。
- `metadata.tmdb.retention_expired` 是媒体库可见的 `library / warning` 通知。它表示某个条目的 TMDB 元数据在最长 180 天保留期内未能重新验证，provider-owned 元数据与缓存已经清除，条目可重新匹配；payload 仅保留本地定位与展示所需的 `media_item_id`、`library_id`、当前 `title`、`provider=tmdb`、`reason_code=tmdb_retention_expired`、`reason_params` 和可选 `diagnostic_message`，不会保留原 TMDB 条目 ID。

### `PUT /api/notifications/{id}/read`

将当前用户可见的一条通知标记为已读。操作幂等；通知不存在或对当前用户不可见时返回 `404`。成功返回 `data: null`，并推进 `user:{id}:notifications` revision。

### `PUT /api/notifications`

批量将当前用户可见通知标记为已读。

请求体：

```json
{"category": "scan"}
```

- `category` 为字符串时只处理该类别。
- `category` 为 `null` 或省略时处理全部类别。
- 返回值 `data` 是本次真正从未读变为已读的记录数；已经读过的通知不会重复写入。
- 只有至少一条通知首次变为已读时才推进 `user:{id}:notifications` revision。

## 4. 服务器媒体目录

### `GET /api/server/media-tree`

作用：
- 查询服务端当前挂载到容器内 `/media` 的递归文件夹树，供创建媒体库时选择 `root_path`

权限：
- 仅 `admin`

返回：
- `200 OK`
- `/media` 存在且为目录时，返回根节点 `MediaDirectoryNodeResponse`
- `/media` 不存在或不是目录时，`data` 返回 `null`

```json
{
  "name": "media",
  "path": "/media",
  "children": [
    {
      "name": "movies",
      "path": "/media/movies",
      "children": []
    },
    {
      "name": "series",
      "path": "/media/series",
      "children": []
    }
  ]
}
```

字段说明：
- `name`：当前文件夹名称
- `path`：容器内绝对路径，可直接作为 `POST /api/libraries` 的 `root_path`
- `children`：子文件夹节点；接口只返回文件夹，不返回普通文件

说明：
- 宿主机媒体根目录在 Docker Compose 的卷挂载中直接配置，并只读挂载到容器内 `/media`；无需创建 `.env`
- 返回树的根节点 `path` 表示客户端当前可见的服务端根目录
- 服务端递归读取全部子文件夹，并按名称排序
- 客户端不得把本机文件系统路径作为服务端 `root_path`

## 5. 媒体库

### `GET /api/libraries`

作用：
- 查询当前用户可见的媒体库

典型场景：
- 前端首页或设置页展示媒体库列表

权限：
- `admin` 返回全部媒体库
- `viewer` 只返回自己被授权的媒体库

返回：
- `200 OK`
- 返回 `LibraryResponse[]`

关键字段：
- `id`：媒体库 ID
- `name`：媒体库名称
- `description`：媒体库描述，可为空
- `metadata_language`：该媒体库扫描和 TMDB 补全时使用的语言，当前支持 `zh-CN` / `en-US`
- `root_path`：扫描根目录

### `GET /api/libraries/recently-added`

作用：
- 查询首页使用的“按库分组的最新添加”数据

权限：
- `admin` 返回全部媒体库中有新增内容的分组
- `viewer` 只返回自己被授权媒体库中有新增内容的分组

查询参数：
- `days`：可选，只返回最近多少天内新增的媒体条目，最大 `365`；不传时不做时间范围过滤
- `limit`：可选，每个媒体库返回多少个媒体条目，默认 `8`，最大 `50`

排序语义：
- 媒体条目按 `media_items.created_at desc, id desc` 排序
- 媒体库分组按各自最近一个媒体条目的 `created_at desc` 排序
- 接口返回全部有内容且当前用户可访问的媒体库分组，不额外限制分组数量
- 查询默认按每个媒体库最新 `8` 条截断，不限制入库时间；显式传入 `days` 时才按时间范围过滤
- 没有可展示媒体条目的库不会出现在返回结果里，前端应显示真实空态，而不是用其他列表接口补一个假分组

返回：
- `200 OK`
- 返回 `RecentlyAddedLibraryMediaItemsResponse[]`

关键字段：
- `library`：当前分组所属媒体库
- `items`：该库内按最新添加顺序截断后的媒体条目
- `total`：该库内符合此接口展示范围的媒体条目总数，不受 `limit` 截断影响

```json
[
  {
    "library": {
      "id": 1,
      "name": "Overseas TV",
      "description": null,
      "metadata_language": "zh-CN",
      "root_path": "/media/overseas-tv",
      "created_at": "2026-06-05T09:00:00+08:00",
      "updated_at": "2026-06-05T09:00:00+08:00"
    },
    "items": [
      {
        "id": 42,
        "library_id": 1,
        "media_type": "series",
        "title": "The Long Voyage",
        "source_title": "The Long Voyage",
        "original_title": null,
        "sort_title": null,
        "metadata_provider": "tmdb",
        "metadata_provider_item_id": "123",
        "metadata_status": "matched",
        "metadata_failure_reason": null,
        "remote_media_type": "series",
        "year": 2023,
        "ratings": [
          {
            "source": "tmdb",
            "kind": "audience",
            "score": 8.6,
            "scale": 10.0,
            "rating_count": 12345,
            "attributes": {},
            "fetched_at": "2026-06-05T09:20:00+08:00"
          }
        ],
        "country": "US",
        "genres": "Drama, Adventure",
        "studio": null,
        "overview": null,
        "poster_path": "/api/media-items/42/poster?v=1780630000",
        "backdrop_path": "/api/media-items/42/backdrop?v=1780630000",
        "logo_path": "/api/media-items/42/logo?v=1780630000",
        "created_at": "2026-06-05T09:12:00+08:00",
        "updated_at": "2026-06-05T09:20:00+08:00"
      }
    ],
    "total": 24
  }
]
```

### `POST /api/libraries`

作用：
- 创建一个新的媒体库

权限：
- 仅 `admin`

请求体：

```json
{
  "name": "Media",
  "description": "家庭影音混合库",
  "metadata_language": "zh-CN",
  "root_path": "/data/media"
}
```

字段说明：
- `name`：媒体库名称
- `description`：可选，媒体库描述
- `metadata_language`：TMDB 元数据语言，支持 `zh-CN` / `en-US`，不传时默认 `zh-CN`
- `root_path`：要扫描的本地目录

关键校验：
- 名称不能为空
- 路径不能为空
- 路径必须存在且必须是目录

返回：
- 成功时 `201 Created`
- 返回创建后的 `LibraryResponse`

说明：
- 创建媒体库后自动触发一次后台扫描，也可显式调用 `POST /api/libraries/{id}/scan`
- 媒体库不提供启用/禁用状态；已创建的库始终可以被手动扫描
- 允许重叠或完全相同的 `root_path`。同一个物理文件如果被多个库路径覆盖，会在各自库里独立建模和展示。
- 媒体库不要求客户端选择电影或剧集类型。名称拆分、季集识别、分组、增量扫描和任务进度见 [`MEDIA_LIBRARY_SCAN.md`](MEDIA_LIBRARY_SCAN.md)。
- TMDB 类型路由、身份匹配、字段覆盖和失败分类见 [`TMDB_INTEGRATION.md`](TMDB_INTEGRATION.md)。

### `GET /api/libraries/{id}`

作用：
- 查询单个媒体库详情

权限：
- 需要当前用户对该媒体库有访问权

路径参数：
- `id`：`library_id`

典型场景：
- 媒体库详情页首屏

返回：
- `200 OK`
- 返回 `LibraryDetailResponse`

关键字段：
- `name`：媒体库名称
- `description`：媒体库描述，可为空
- `media_count`：当前库中的媒体数量
- `movie_count`：当前库中的电影数量
- `series_count`：当前库中的剧集数量；单集不单独计入该字段
- `last_scan`：最近一次扫描摘要，没有时为 `null`
- `last_scan.phase`：持久化的最近扫描阶段，使用 `discovering` / `processing` / `finalizing` / `finished`，尚未被 worker 领取的 `pending` 任务为 `null`；服务重启后可通过 HTTP 恢复
- `last_scan.progress_percent`：与扫描任务接口和 SSE 相同的服务端任务级权威进度；客户端从任意入口恢复后都直接使用该值

### `DELETE /api/libraries/{id}`

作用：
- 删除一个媒体库

权限：
- 仅 `admin`

路径参数：
- `id`：`library_id`

典型场景：
- 用户确认不再需要某个媒体库
- 清理误建库或错误路径配置

返回：
- 删除成功时返回 `200 OK`
- 返回统一 envelope，`message` 为 `library deleted`，`data` 为 `null`

说明：
- 删除前服务会先把该库标记为“正在删除”，阻止新的扫描请求进入
- 如果当前进程有正在执行的扫描任务，服务会先请求取消并等待它退出；删除事务还会把其它 worker 实例持有的同库扫描任务标记为取消
- 删除事务只删除 `libraries` 权威记录；扫描任务、授权关系、媒体条目、资源文件、字幕、音轨、季集、演员、评分、外部 ID、通知和播放进度全部依靠数据库外键 `ON DELETE CASCADE` 清理
- 同一个数据库事务会持久化一条 `library.cache.cleanup` 后台任务。事务提交后 API 即返回成功，后台 worker 再删除 `MOVA_CACHE_DIR/libraries/{library_id}` 完整缓存命名空间
- 每个媒体库的 TMDB 图片、WebVTT 字幕和音轨 remux 缓存都位于自己的库命名空间；媒体目录及其中的 NFO、sidecar 图片和字幕不会被修改
- 缓存清理最多尝试 10 次。服务重启或 worker 租约过期后任务会继续执行；重试耗尽时管理员通知中心会出现 `cache.cleanup.failed`
- 如果同一时间重复删除同一个库，或扫描仍在停止过程中，会返回 `409 Conflict`
- 删除事务、worker 协调、缓存目录边界和失败恢复的完整约束见 [`LIBRARY_CACHE_LIFECYCLE.md`](LIBRARY_CACHE_LIFECYCLE.md)。

### `PATCH /api/libraries/{id}`

作用：
- 更新媒体库基础配置

权限：
- 仅 `admin`

路径参数：
- `id`：`library_id`

请求体：

```json
{
  "name": "Movies HD",
  "description": "4K 电影库",
  "metadata_language": "en-US"
}
```

字段说明：
- `name`：可选，更新媒体库名称
- `description`：可选，更新媒体库描述；传 `null` 可清空现有描述
- `metadata_language`：可选，更新 TMDB 元数据语言，支持 `zh-CN` / `en-US`

返回：
- 成功时 `200 OK`
- 返回更新后的 `LibraryResponse`

说明：
- 至少要传一个字段，否则返回 `400 Bad Request`
- 只更新名称或描述不会触发扫描
- 当 `metadata_language` 发生变化时，服务端会先停止该库当前正在执行的扫描，把库内所有媒体条目标记为 `metadata_status = pending`，然后自动创建一次覆盖全库的元数据扫描；文件未变化时会复用既有本地分析、音轨和字幕结果，但会按新语言重新请求全部远端元数据
- 语言配置、条目待处理状态、语言相关缓存失效、catalog revision 和新扫描任务在同一数据库事务中提交；任一步失败都会整体回滚
- 如果已有扫描无法在安全窗口内停止，或数据库中仍存在其它 worker 持有的活跃扫描，返回 `409 Conflict`，且不提交配置变更
- 媒体库不提供启用/禁用状态，更新接口不接受该字段

### `GET /api/libraries/{id}/media-items`

作用：
- 查询某个媒体库下已经扫描入库的媒体条目列表

路径参数：
- `id`：`library_id`

典型场景：
- 媒体库内容列表页

查询参数：
- `page`：可选，页码，默认 `1`
- `page_size`：可选，每页条数，默认 `50`，最大 `100`
- `query`：可选，按标题筛选，会匹配 `title`、`source_title` 和 `original_title`
- `year`：可选，按发行年精确筛选
- `category`：可选，服务端定义的目录分类，支持 `movie` / `series` / `needs_review`；不传时返回全部顶层条目
- `sort_by`：可选，排序字段，支持 `title` / `year` / `rating`，默认 `title`
- `sort_order`：可选，排序方向，支持 `asc` / `desc`，默认 `asc`

返回：
- `200 OK`
- 返回：

```json
{
  "items": [],
  "total": 0,
  "page": 1,
  "page_size": 50
}
```

说明：
- 列表返回顶层媒体条目，即电影和剧；剧集的单集不会直接出现在这个列表里。分类筛选、名称与年份筛选、排序和分页全部由服务端按此顺序处理，客户端不得把分页结果重新拆组后解释为全局排名
- `category` 是服务端根据条目类型和元数据处理状态计算的投影，不是 `media_type` 的别名。`pending` 和已成功确认的条目按条目 `media_type` 进入 `movie` / `series`；`metadata_status` 为 `skipped` / `unmatched` / `failed`，或已有远端类型与本地类型冲突的条目进入 `needs_review`
- 客户端不得根据 `metadata_status`、`remote_media_type` 或当前分页结果重新推导分类；筛选、计数、排序和分页均以服务端结果为准
- 默认按名称升序返回；名称排序依次使用 NFO `sort_title`、展示标题 `title` 和源标题 `source_title`。所有排序都追加稳定的名称和条目 ID 次级顺序，因此分页过程中不会因同分或同年份随机换位
- `year` 和 `rating` 缺失的条目始终排在有值条目之后，不受升序或降序影响
- `rating` 使用服务端定义的首选评分：手动值优先于 NFO，NFO 优先于远端值；同级来源优先 TMDB，再按来源和评分类型稳定排序。客户端使用同一顺序选择卡片主评分。排序前会按 `score / scale` 归一化，不会把不同满分制的原始分数直接比较
- 查询参数支持标题筛选和发行年筛选；筛选、排序在分页之前由服务端对完整结果集执行
- 不支持的 `category`、`sort_by`、`sort_order`，以及非正数 `year` 返回 `400 Bad Request`

### `GET /api/libraries/{id}/scan-jobs`

权限：
- 仅 `admin`

作用：
- 查询某个媒体库的扫描历史

路径参数：
- `id`：`library_id`

典型场景：
- 调试
- 排障
- 查看扫描历史记录

返回：
- `200 OK`
- 返回 `ScanJobResponse[]`

说明：
- 按创建时间倒序返回

### `GET /api/libraries/{id}/scan-jobs/{scan_job_id}`

权限：
- 仅 `admin`

作用：
- 查询某个媒体库下的单个扫描任务状态

路径参数：
- `id`：`library_id`
- `scan_job_id`：扫描任务 ID

典型场景：
- 前端轮询扫描进度

返回：
- `200 OK`
- 返回 `ScanJobResponse`

关键字段：
- `status`：`pending` / `running` / `success` / `failed` / `cancelled`
- `phase`：持久化扫描阶段，使用 `discovering` / `processing` / `finalizing` / `finished`；尚未被 worker 领取或正在等待后台重试的 `pending` 任务为 `null`
- `scanned_files`：已发现文件数
- `total_files`：已知总文件数
- `local_analyzed_files`：已完成完整本地分析并通过扫描组检查点持久化的物理文件数；此时 pending 媒体事务可能尚未提交
- `local_committed_files`：已通过组级短事务写入 pending 数据的物理文件数
- `remote_completed_files`：已完成 TMDB/图片处理并写入远端业务终态的物理文件数
- `progress_percent`：服务端持久化的任务级权威进度，范围为 0～100 且不会回退；运行中最大 99，只有任务成功写入终态时为 100。计数权重和并行推进规则见 [`MEDIA_LIBRARY_SCAN.md`](MEDIA_LIBRARY_SCAN.md)
- `error_message`：带阶段上下文的失败原因，例如：
  - `Directory scan failed: Failed to scan media directory /media/movies: ...`
  - `Media processing failed: Failed to process scan pipeline: ...`
  - `Library finalization failed: Failed to save changed library data`

等待重试的 `pending` 任务也会暂存最近一次执行的 `error_message` 和最后权威进度，但它还不是终态；下一次 worker 领取时会清除该错误并继续执行。重试额度耗尽后才写入 `failed / finished`。

### `POST /api/libraries/{id}/scan`

扫描工作流、名称拆分、分组、事务和 TMDB 调用规则见 [`MEDIA_LIBRARY_SCAN.md`](MEDIA_LIBRARY_SCAN.md)。

权限：
- 仅 `admin`

作用：
- 为指定媒体库创建异步扫描任务

路径参数：
- `id`：`library_id`

典型场景：
- 用户点击“开始扫描”

返回：
- 如果创建了新任务：`202 Accepted`
- 如果当前库已有活跃任务并被复用：`200 OK`
- 响应体均为 `ScanJobResponse`
- 如果媒体库正在删除：`409 Conflict`

说明：
- 媒体库存在 `pending` 或 `running` 任务时复用该任务，不启动第二个扫描
- 扫描请求和 PostgreSQL `background_jobs` 后台任务在同一事务内持久化；服务重启后 worker 重新领取未完成任务。客户端可以通过 `/api/libraries/{id}/scan-jobs/{scan_job_id}`、realtime state 和临时扫描事件读取进度
- 创建媒体库触发首次扫描；之后的新增、删除、改名和移动通过手动扫描收敛
- 扫描是按库串行、可恢复的后台任务。增量复用、worker 流水线、组级事务、进度算法和 finalize 规则统一见 [`MEDIA_LIBRARY_SCAN.md`](MEDIA_LIBRARY_SCAN.md)。

### `GET /api/search`

作用：
- 在当前用户可见的媒体库中做全局模糊搜索

典型场景：
- 搜索页面输入时，搜索电影、剧集条目和本地可用的集条目

权限：
- 需要登录态
- `admin` 搜索全部媒体库
- `viewer` 只搜索自己被授权的媒体库

查询参数：
- `q`：搜索关键字；空白时返回空数组
- `limit`：可选，返回结果上限，默认 `12`，最大 `30`

匹配范围：
- 电影 / 剧集条目：匹配 `title`、`source_title`、`original_title`
- 集条目：匹配集标题、本地集条目标题、本地集条目源标题、剧集标题、剧集源标题和原始标题

返回：
- `200 OK`
- 返回 `GlobalSearchResultResponse[]`

关键字段：
- `kind`：`media_item` 或 `episode`
- `media_item_id`：点击结果时应打开的本地媒体条目 ID；集条目返回对应本地集条目的 `media_item_id`
- `series_media_item_id`：只有 `kind = episode` 时返回所属剧集 ID
- `library_id` / `library_name`：结果所属媒体库
- `poster_path` / `backdrop_path`：只来自该搜索结果自身记录；没有值时保持 `null`，不会使用其他层级图片兜底
- `season_number` / `episode_number`：只有集条目有值
- `ratings`：搜索结果自身的来源原生评分数组，结构与媒体条目详情一致；当前已匹配的电影和剧集通常包含 TMDB 评分，未获取到有效投票时为空数组

## 6. 媒体条目

### `GET /api/media-items/{id}`

作用：
- 查询单个媒体条目详情
- 返回基础元数据，让详情页主体可以尽快渲染

路径参数：
- `id`：`media_item_id`

典型场景：
- 媒体详情页

返回：
- `200 OK`
- 返回 `MediaItemDetailResponse`

说明：
- 这里的 `id` 是 `media_item_id`
- 不是 `library_id`

关键字段：
- `title`：当前前端默认展示名；TMDB 命中后优先使用当前媒体库语言对应的标题
- `source_title`：文件名解析出的原始资源名，主要用于元数据匹配和问题排查，不建议直接作为前端展示名
- `metadata_provider` / `metadata_provider_item_id`：远端 metadata binding，表示条目绑定到具体远端条目。提供商 ID 以字符串传输和存储，客户端不得假设它一定是数字
- `metadata_status`：使用 `pending` / `matched` / `unmatched` / `failed` / `skipped`；`pending` 表示扫描中的远端确认中间态
- `metadata_failure_reason`：远端处理原因。常见组合为 `unmatched + no_remote_match`、`failed + metadata_provider_error`、`skipped + metadata_provider_disabled`；已有 binding 在临时 provider 故障时可以保留为 `matched + metadata_provider_error`。正常 `pending` 或成功 `matched` 为 `null`
- `remote_media_type`：使用 `movie` / `series`；没有远端判断或 TMDB 未启用时为 `null`
- `tagline`：可选宣传语；可以来自选中 NFO 或其它已确认来源
- `premiere_date`：可选首映/首播日期，格式为 `YYYY-MM-DD`；单集优先使用 NFO `aired`
- `content_rating`：可选内容分级，例如 `PG-13` 或 `TV-14`
- `ratings`：评分数组；`source` 是评分品牌，`kind` 是 `audience` 或 `critic`，`retrieved_via` 标识实际写入来源（当前自动流程使用 `nfo` 或 `tmdb`），`score` 与 `scale` 保留来源原始量纲，`rating_count` 是投票/评价数量；无有效评分时返回空数组
- `country`：可选的国家/地区信息；电影会优先使用 TMDB 的 production countries，剧集会优先使用 TMDB 的 origin country；服务端按自由文本存储，不做 255 字符截断
- `genres`：可选的题材类型字符串；来自 TMDB genres，会按展示顺序拼接；服务端按自由文本存储，不做 255 字符截断
- `studio`：可选的制作公司字符串；来自 TMDB production companies，会按展示顺序拼接；服务端按自由文本存储，不做 255 字符截断
- `overview`：简介，可来自本地 sidecar `.nfo` 或 TMDB
- `poster_path`：海报可访问 URL；TMDB 图片会优先缓存到本地，因此通常是 `/api/media-items/{id}/poster`
- `backdrop_path`：背景图可访问 URL；TMDB 图片会优先缓存到本地，因此通常是 `/api/media-items/{id}/backdrop`
- `logo_path`：透明标题 Logo 可访问 URL；没有合适素材时为 `null`。TMDB 素材会优先缓存到本地，因此通常是 `/api/media-items/{id}/logo`
- 图片字段只会返回 Mova 内部图片路由或不带 query/fragment 的 TMDB 官方 HTTPS 图片地址；历史数据库中的任意第三方、localhost、私网或其他不可信远程地址会被隐藏为 `null`

返回示例：

```json
{
  "id": 3,
  "library_id": 1,
  "media_type": "series",
  "title": "Arcane",
  "source_title": "Arcane",
  "original_title": "Arcane",
  "sort_title": null,
  "metadata_provider": "tmdb",
  "metadata_provider_item_id": "94605",
  "metadata_status": "matched",
  "metadata_failure_reason": null,
  "remote_media_type": "series",
  "year": 2021,
  "tagline": "Every legend has a beginning.",
  "premiere_date": "2021-11-06",
  "content_rating": "TV-14",
  "ratings": [
    {
      "source": "tmdb",
      "kind": "audience",
      "retrieved_via": "tmdb",
      "score": 9.0,
      "scale": 10.0,
      "rating_count": 24680,
      "attributes": {},
      "fetched_at": "2026-03-24T12:00:00+08:00"
    }
  ],
  "country": "US",
  "genres": "Animation · Action & Adventure · Sci-Fi & Fantasy",
  "studio": "Fortiche Production",
  "overview": "……",
  "poster_path": "/api/media-items/3/poster",
  "backdrop_path": "/api/media-items/3/backdrop",
  "logo_path": "/api/media-items/3/logo",
  "created_at": "2026-03-24T12:00:00+08:00",
  "updated_at": "2026-03-24T12:00:00+08:00"
}
```

### `GET /api/media-items/{id}/metadata-sources`

作用：
- 查询一个媒体条目的来源身份、持久化演职员和本地元数据来源摘要
- 摘要查询不会从数据库读取标准化 `payload`，也不会访问文件系统或解析 NFO
- 适用于元数据诊断入口和管理界面；普通详情页不需要主动请求

权限：
- 仅管理员可用
- 管理员仍必须能访问条目所属媒体库

关键字段：
- `external_ids`：作品外部身份数组；每项包含 `provider`、`external_id` 和 `retrieved_via`。同一 provider 可以保留不同实际来源，当前自动写入来源为 `nfo` 或 `tmdb`；客户端不得把 episode ID 当作父剧 ID
- `credits`：持久化演职员数组；`credit_type` 使用 `actor` / `director` / `writer`，并返回 `retrieved_via`、`sort_order`、可选 `person_id`、姓名、角色和头像路径。actor NFO 中的 TMDB ID 可以成为 `person_id`，通过安全校验的远程头像可以成为 `profile_path`；服务端不在持久化层截断 NFO 演职员
- `local_metadata_sources`：本地元数据来源摘要数组，选中来源排在前面；每项包含稳定的 `id`、`source_path`、`document_type`、`schema_version`、`is_locked`、`is_selected` 和时间
- `is_locked` 只回显 NFO `lockdata` 兼容标记；当前服务端没有逐字段人工锁，也不依据该值阻止字段刷新

返回示例：

```json
{
  "external_ids": [
    {
      "provider": "tmdb",
      "external_id": "94605",
      "retrieved_via": "tmdb"
    },
    {
      "provider": "imdb",
      "external_id": "tt11126994",
      "retrieved_via": "nfo"
    }
  ],
  "credits": [
    {
      "credit_type": "actor",
      "retrieved_via": "nfo",
      "sort_order": 0,
      "person_id": "1356210",
      "name": "Hailee Steinfeld",
      "role": "Vi",
      "profile_path": null
    }
  ],
  "local_metadata_sources": [
    {
      "id": 17,
      "source_path": "/media/Arcane/tvshow.nfo",
      "document_type": "tvshow",
      "schema_version": 1,
      "is_locked": false,
      "is_selected": true,
      "created_at": "2026-03-24T12:00:00+08:00",
      "updated_at": "2026-03-24T12:00:00+08:00"
    }
  ]
}
```

### `GET /api/media-items/{id}/metadata-sources/{source_id}`

作用：
- 查询一个已持久化本地元数据来源的完整标准化 `payload`
- 请求时只观察并解析所选的一个 NFO，用于显示当前文件状态
- 不调用 ffprobe 或 TMDB，也不会创建、更新或删除来源记录

权限：
- 仅管理员可用
- 管理员仍必须能访问条目所属媒体库
- `source_id` 必须属于路径中的 `media_item_id`，否则返回 `404 Not Found`

关键字段：
- `id`：来源记录稳定 ID，由摘要接口返回
- `payload`：最近一次成功扫描/刷新后持久化的版本化标准结构，只包含服务端识别字段，不返回原始 XML、未知标签、DTD 或实体
- 标准 payload 支持标题/排序标题/正式与自定义分级的独立语义、作品语言与元数据语言、旧式 `id`、结构化评分、actor IDs / profile、季标题/简介/图片、类型不丢失的 artwork，以及 `genre` / `country` / writer 的 `/` 分隔写法；完整字段和投影范围见 [`NFO_METADATA.md`](NFO_METADATA.md)
- `observation_status`：本次请求对 `source_path` 的实时观察，使用 `valid` / `invalid` / `missing`；观察以条目所属媒体库根目录为边界，只读取边界内的非符号链接 NFO
- `observation_error_code`：仅在 `observation_status = invalid` 时提供，可能值为 `open_failed`、`inspect_failed`、`not_regular_file`、`too_large`、`read_failed`、`grew_beyond_limit`、`invalid_utf8`、`forbidden_xml_declaration`、`malformed_xml`、`unsupported_root`、`unexpected_root_kind`、`outside_library_root`、`symlink_not_allowed`、`secure_open_unavailable`、`resource_limit_exceeded` 或 `unsupported_document_type`。`resource_limit_exceeded` 表示整份 NFO 超过结构化解析上限，服务端不会截断后使用；响应仍可同时携带最近一次有效的 `payload`
- `invalid` / `missing` 可以与旧 `payload` 同时出现：前者表示当前文件状态，后者表示最近一次成功保存的内容。新放入但尚未扫描或刷新的 NFO 不会因为查询本接口自动建立来源记录

返回示例：

```json
{
  "id": 17,
  "source_path": "/media/Arcane/tvshow.nfo",
  "document_type": "tvshow",
  "schema_version": 1,
  "is_locked": false,
  "is_selected": true,
  "observation_status": "valid",
  "observation_error_code": null,
  "payload": {
    "schema_version": 1,
    "metadata": {
      "kind": "tv_show",
      "title": "Arcane",
      "year": 2021,
      "genres": ["Animation", "Drama"],
      "unique_ids": [
        {
          "provider": "tmdb",
          "value": "94605",
          "is_default": true
        }
      ]
    }
  },
  "created_at": "2026-03-24T12:00:00+08:00",
  "updated_at": "2026-03-24T12:00:00+08:00"
}
```

### `GET /api/media-items/{id}/cast`

作用：
- 查询单个媒体条目的完整演员列表
- 已选中的手动 / NFO 演员是该条目的权威本地来源；只要该来源含有效演员，就完整返回它，不与提供方演员混合
- 没有有效的已选本地演员时，服务端读取仍在有效期内的提供方演员缓存
- 本地来源和缓存都没有演员信息时，会在这个请求里按需拉一次远端演员并直接写库
- 服务端保存并返回元数据提供方返回的全部有效演员，不按人数截断
- 本地来源与提供方缓存分别保留来源所有权；后续切换本地来源或刷新远端缓存时，不会互相覆盖
- 拉取失败不会阻断详情页，其它主体信息仍可正常展示；只是这次演员列表可能为空

路径参数：
- `id`：`media_item_id`

典型场景：
- 详情页在主体信息已经渲染后，再异步加载演员区

返回：
- `200 OK`
- 返回 `MediaCastMemberResponse[]`

返回示例：

```json
[
  {
    "person_id": "12345",
    "sort_order": 0,
    "name": "Ella Purnell",
    "character_name": "Jinx",
    "profile_path": "https://image.tmdb.org/t/p/original/xxx.jpg"
  }
]
```

### `GET /api/media-items/{id}/playback-header`

作用：
- 查询播放器页左上角需要的头部信息

说明：
- 电影返回电影标题
- 单集返回“剧名 + 季集号 + 单集标题”所需的结构化字段
- `series_media_item_id`：电影返回 `null`；单集返回所属剧集的 `media_item_id`，客户端据此读取剧集大纲或返回剧集详情
- `logo_path` 返回当前作品的透明标题 Logo；播放电影时属于电影条目，播放单集时属于其剧集条目。缺失时客户端回退文字标题
- 如果该条目已经完成 TMDB 元数据增强，这里的标题会优先使用增强后的标题
- 播放本地剧集且当前没有可用片头区间时，请求路径只执行一次轻量、幂等的 season 级持久化任务入队；FFmpeg、输入指纹计算、代表集分析和重试均由 worker 执行，不阻断本次播放
- 检测完成后服务端推进该库的 catalog revision；Web、macOS 和 iOS 客户端按 SSE 资源失效规则重新读取播放头与 `episode-outline`。首次响应仍可能没有片头区间
- 完整触发条件、算法阈值、失效规则和资源上限见 [`INTRO_DETECTION.md`](INTRO_DETECTION.md)

返回示例：

```json
{
  "media_item_id": 42,
  "library_id": 1,
  "media_type": "episode",
  "series_media_item_id": 7,
  "title": "Severance",
  "original_title": "Severance",
  "year": 2022,
  "logo_path": "/api/media-items/7/logo?v=1780630000",
  "season_number": 1,
  "episode_number": 7,
  "episode_title": "Defiant Jazz"
}
```

### `GET /api/media-items/{id}/files`

作用：
- 查询某个媒体条目关联的物理文件列表

路径参数：
- `id`：`media_item_id`

典型场景：
- 播放前拿 `media_file_id`
- 多版本文件切换

返回：
- `200 OK`
- 返回 `MediaFileResponse[]`

关键字段：
- `id`：`media_file_id`
- `media_item_id`：所属媒体条目
- `source_kind`：播放源类型，`local_file` 表示本地媒体文件，`strm` 表示由本地 `.strm` 载体引用的 HTTP(S) 远程流
- `file_path`：后端内部文件路径
- `file_size`：文件或 STRM 本地引用载体的字节数
- `container`：容器格式，如 `mp4` / `mkv`
- `duration_seconds` / `video_codec` / `audio_codec` / `width` / `height` / `bitrate`：基础探测字段
- `video_title` / `video_profile` / `video_level`：视频流标题、profile、level
- `video_bitrate` / `video_frame_rate` / `video_aspect_ratio` / `video_scan_type`：视频码率、帧率、宽高比、扫描类型
- `video_color_primaries` / `video_color_space` / `video_color_transfer`：色彩原色、色域、传递特性
- `video_bit_depth` / `video_pixel_format` / `video_reference_frames`：位深、像素格式、参考帧
- `technical_tags`：从 `ffprobe` 探测结果归一化出来的资源技术标签，例如 `HDR10`、`HDR10+`、`Dolby Vision`、`HLG`、`DTS`、`DTS-HD`、`Atmos`
- `scan_hash`：服务端增量扫描使用的不透明指纹，可能为 `null`；客户端只能回显或忽略，不能解析、比较其内部格式或据此决定播放与刷新行为

说明：
- 客户端播放前应先从这个接口取得 `media_file_id`
- `source_kind = strm` 时，`file_path` 和 `file_size` 分别表示本地 `.strm` 载体路径与载体大小，不是远端 URL 和远端媒体大小；API 不返回原始或重定向后的远端 URL
- STRM 扫描不访问远端且不执行 `ffprobe`，所以容器、时长、编码、码率、分辨率、内嵌音轨和技术标签均为空；客户端应显示远程流来源，不应把空字段伪装成本地技术参数
- 对 `source_kind = local_file` 的文件，如果服务运行环境里安装了 `ffprobe`，扫描时会尽量填充时长、编码、分辨率、码率和 `technical_tags`
- `technical_tags` 是文件维度字段；同一个电影或单集有多个版本时，每个 `media_file` 可以返回不同标签
- 如果没有安装 `ffprobe`，或者文件探测失败，这些字段会保持为空，但不会阻断扫描
- 如果这个条目是 `series`，这里通常返回空列表；季集层级和本地可用性统一改用 `/api/media-items/{id}/episode-outline`

### `GET /api/media-items/{id}/episode-outline`

作用：
- 查询剧集“全集大纲 + 本地可用性”
- 客户端通过该接口统一读取季、集层级数据

路径参数：
- `id`：`series media_item_id`

返回：
- `200 OK`
- 返回对象结构：
  - `seasons[]`
  - `seasons[].season_id`（本地已有该季时有值）
  - `seasons[].season_number`
  - `seasons[].title`
  - `seasons[].year`
  - `seasons[].overview`
  - `seasons[].poster_path`
  - `seasons[].intro_start_seconds`
  - `seasons[].intro_end_seconds`
  - `seasons[].episodes[]`
  - `seasons[].episodes[].episode_number`
  - `seasons[].episodes[].title`
  - `seasons[].episodes[].overview`
  - `seasons[].episodes[].poster_path`
  - `seasons[].episodes[].backdrop_path`
  - `seasons[].episodes[].intro_start_seconds`
  - `seasons[].episodes[].intro_end_seconds`
  - `seasons[].episodes[].media_item_id`（本地存在时有值）
  - `seasons[].episodes[].is_available`（本地存在时为 `true`）
  - `seasons[].episodes[].playback_progress`
  - `seasons[].episodes[].playback_progress.last_media_file_id`
  - `seasons[].episodes[].playback_progress.position_seconds`
  - `seasons[].episodes[].playback_progress.duration_seconds`
  - `seasons[].episodes[].playback_progress.last_watched_at`
  - `seasons[].episodes[].playback_progress.is_finished`

说明：
- 接口读取 TMDB 剧集大纲，并与本地已入库集进行合并。
- 返回结果只包含“至少有一集本地资源”的季；纯远端季不会出现在 `seasons[]` 中。
- TMDB 不可用或匹配失败时，会退化为仅返回本地已入库集。
- TMDB 提供季海报（`season poster`）和集剧照（`episode still`）；剧集大纲中的季只返回 `poster_path`，页面背景使用剧集条目自身的 `backdrop_path`，集剧照只写入集级 `poster_path`。
- 若集级图片缺失，后端保持为空；不会尝试从本地视频抽取第一帧回退，也不会把通用目录海报（如 `poster.jpg` / `folder.jpg`）、季图或剧图误当成单集封面。
- `seasons[].intro_start_seconds` / `seasons[].intro_end_seconds` 承载按需检测并持久化的 season 级片头区间；`episodes[].intro_*` 默认为空。输入变化和算法版本规则见 [`INTRO_DETECTION.md`](INTRO_DETECTION.md)。
- `episodes[].playback_progress` 会带上该集最近一次播放快照；`last_media_file_id` 用于在同一集存在多个物理版本时恢复最近播放的版本。前端可以据此显示集卡进度、已看完状态，以及“最近一集已播完则默认跳下一集”的续播入口。
- 可直接用于前端“可播放集高亮、缺失集置灰”的展示逻辑。
- TMDB 剧集大纲缓存在 PostgreSQL `series_episode_outline_cache` 的 `jsonb` 文档中，数据库会拒绝无效 JSON；默认 TTL 为 24 小时。
- 缓存过期且 TMDB 临时不可用时，接口返回最近一次可用缓存。

### `GET /api/media-items/{id}/metadata-search`

作用：
- 管理员手动输入资源名称和年份后，搜索当前媒体条目的候选远端元数据

权限：
- 仅 `admin`

路径参数：
- `id`：`media_item_id`

查询参数：
- `query`：必填，搜索名称
- `year`：可选，搜索年份

说明：
- 人工匹配支持 `movie` 和 `series`；`episode` 不支持单独匹配
- 搜索时会沿用当前媒体库配置的 `metadata_language`
- 如果当前条目已经有 `source_title`，前端通常应优先用它预填搜索框，而不是直接用当前展示标题
- 搜索类型会跟随当前媒体条目的媒体类型：
  - 电影只搜电影
  - 剧只搜剧

返回：
- `200 OK`
- 返回 `MetadataMatchCandidateResponse[]`

返回示例：

```json
[
  {
    "provider_item_id": "1100988",
    "title": "创：战神",
    "original_title": "TRON: Ares",
    "year": 2025,
    "overview": "……",
    "poster_path": "https://image.tmdb.org/t/p/original/xxx.jpg",
    "backdrop_path": "https://image.tmdb.org/t/p/original/yyy.jpg"
  }
]
```

### `POST /api/media-items/{id}/metadata-match`

作用：
- 管理员从候选列表中选中一个结果，并把它替换为当前媒体条目的正式元数据

权限：
- 仅 `admin`

路径参数：
- `id`：`media_item_id`

请求体：

```json
{
  "provider_item_id": "1100988"
}
```

说明：
- 选中的 TMDB 条目 ID 持久化到 `media_items.metadata_provider_item_id`，并将 `metadata_status` 更新为 `matched`
- 演员数据和剧集 outline 按该 TMDB ID 获取，不执行模糊搜索
- 命中的远程图片缓存到本地后写回；选中条目没有 `poster_path` / `backdrop_path` 时对应字段保持为空
- 如果当前条目是剧集，确认替换后会立即拉取该剧的远端季 / 集大纲，并把本地已存在季、已存在集的标题、简介、季海报和集封面写回数据库；远端季 / 集图会先缓存到本地再覆盖旧图，远端缺图时对应字段会清空
- 当前若所属媒体库正在扫描或正在删除，会返回 `409 Conflict`

返回：
- 成功时 `200 OK`
- 返回更新后的 `MediaItemResponse`

### `POST /api/media-items/{id}/refresh-metadata`

权限：
- 仅 `admin`

作用：
- 手动重拉单个媒体条目的 metadata

路径参数：
- `id`：`media_item_id`

典型场景：
- 更新了本地 `.nfo` / `poster.jpg` 后重新同步
- 想让某条内容重新拉一次 TMDB，而不是整库重扫

返回：
- 成功时 `200 OK`
- 返回更新后的 `MediaItemResponse`

说明：
- 这个动作会重新读取该媒体条目关联的源文件、本地 sidecar 和本地图片文件
- 单条刷新会枚举该逻辑条目的全部本地载体：movie / episode 使用自身全部版本，series 使用全部本地季集文件作为 `tvshow.nfo` 查找锚点；NFO 来源去重后统一选源，只有代表文件执行 ffprobe，避免 series 刷新重复探测整剧
- 已有 `matched` TMDB binding 时直接按该 ID 刷新，NFO 或目录中的冲突 ID 只保留为来源信息，不能静默换绑。只有 `POST /api/media-items/{id}/metadata-match` 可以显式替换 binding
- 有效、无效和不存在的 NFO 分开处理：无效来源保留 last-known-good；权威载体集合确认来源不存在后才删除旧快照并提升剩余兼容来源
- 如果 TMDB provider 可用，会保留本地文件结构与 `source_title`，并按自动补全策略应用远端字段；自动扫描与人工替换的不同覆盖强度见 [`TMDB_INTEGRATION.md`](TMDB_INTEGRATION.md)
- 刷新后会同步更新 `metadata_status`、`metadata_failure_reason` 和 `remote_media_type`
- 命中远程图片后，服务端先缓存到本地，再写回 `poster_path` / `backdrop_path` / `logo_path`；远端缺失的图片字段保持为空，禁止使用同条目的其他图片字段或其他层级图片补齐
- 媒体条目通过 `POST /api/media-items/{id}/metadata-match` 绑定精确 TMDB 条目时，演员数据和剧集 outline 使用该 binding
- 源文件被重命名、移动或删除时返回 `409 Conflict` 并要求重新扫描
- 所属媒体库正在扫描或删除时返回 `409 Conflict`
- 接口刷新一个逻辑媒体项及其本地 NFO 来源集合，不提供整库级 metadata refresh

### `GET /api/media-items/{id}/poster`

作用：
- 返回媒体条目的海报图片文件

路径参数：
- `id`：`media_item_id`

典型场景：
- 详情页或列表页展示封面图

返回：
- 成功时返回 `200 OK`
- 响应体为图片内容，不是 JSON

说明：
- 服务本地 sidecar 图片以及已缓存到本地的 TMDB 图片
- 如果极少数情况下缓存失败，详情接口里的 `poster_path` 仍可能是远程 TMDB 图片地址；这时前端应直接使用那个 URL，不需要再请求本接口
- 本地文件必须位于该条目所属媒体库根目录或该库独立的图片缓存目录内，并通过 20 MiB 大小上限和图片文件头校验；历史越界路径、符号链接逃逸和伪装文件返回 `404 Not Found`
- 如果该媒体条目没有海报，返回 `404 Not Found`

### `GET /api/media-items/{id}/backdrop`

作用：
- 返回媒体条目的背景图文件

路径参数：
- `id`：`media_item_id`

典型场景：
- 详情页头图或背景氛围图

返回：
- 成功时返回 `200 OK`
- 响应体为图片内容，不是 JSON

说明：
- 服务本地 sidecar 图片以及已缓存到本地的 TMDB 图片
- 如果极少数情况下缓存失败，详情接口里的 `backdrop_path` 仍可能是远程 TMDB 图片地址；这时前端应直接使用那个 URL，不需要再请求本接口
- 本地文件必须位于该条目所属媒体库根目录或该库独立的图片缓存目录内，并通过 20 MiB 大小上限和图片文件头校验；历史越界路径、符号链接逃逸和伪装文件返回 `404 Not Found`
- 如果该媒体条目没有背景图，返回 `404 Not Found`

### `GET /api/media-items/{id}/logo`

作用：
- 返回电影或剧集的透明标题 Logo 图片文件

路径参数：
- `id`：拥有该 Logo 的 `media_item_id`；单集播放头部返回的 URL 会自动指向对应剧集条目

典型场景：
- 播放页头部用作品 Logo 替代纯文字标题

返回：
- 成功时返回 `200 OK`
- 响应体为图片内容，不是 JSON

说明：
- 服务已缓存到本地的 TMDB Logo
- 缓存失败时 `logo_path` 可能保留远程 TMDB 图片地址，客户端直接使用该 URL
- 本地 Logo 使用与海报、背景图相同的媒体库边界、大小和图片内容校验
- 没有合适 Logo 时返回 `404 Not Found`，客户端必须回退文字标题

### `GET /api/seasons/{id}/poster`

作用：
- 返回某一季的海报图片文件

路径参数：
- `id`：`season_id`

返回：
- 成功时返回 `200 OK`
- 响应体为图片内容，不是 JSON

说明：
- 服务本地缓存图片或 sidecar 图片
- 如果 `poster_path` 是远程 URL，前端应直接使用 URL，不需要再请求本接口
- 本地季海报使用所属媒体库边界、20 MiB 大小上限和图片文件头校验
- 如果该季没有海报，返回 `404 Not Found`

### `GET /api/seasons/{id}/backdrop`

作用：
- 返回某一季的背景图文件

路径参数：
- `id`：`season_id`

返回：
- 成功时返回 `200 OK`
- 响应体为图片内容，不是 JSON

说明：
- 服务本地缓存图片或 sidecar 图片
- 如果 `backdrop_path` 是远程 URL，前端应直接使用 URL，不需要再请求本接口
- 本地季背景图使用所属媒体库边界、20 MiB 大小上限和图片文件头校验
- 如果该季没有背景图，返回 `404 Not Found`

## 7. 播放进度

### `GET /api/media-items/{id}/playback-progress`

作用：
- 查询某个媒体条目的最近播放进度

路径参数：
- `id`：`media_item_id`

典型场景：
- 进入播放页时恢复到上次位置

返回：
- `200 OK`
- 有记录时返回 `PlaybackProgressResponse`
- 没有记录时返回 `null`

关键字段：
- `last_media_file_id`：最近播放的有效文件 ID；原版本被删除但同一条目仍有其它文件时，服务端按文件列表顺序返回首个现存版本，只有没有任何可用文件时才为 `null`
- `position_seconds`：当前记录的播放秒数
- `duration_seconds`：记录的总时长
- `last_watched_at`：最近一次上报时间
- `is_finished`：是否标记为已看完

说明：
- `null` 是这个接口的正常语义，表示“当前用户还没有这条内容的播放记录”，不应当被当成异常
- 播放进度以 `(user_id, media_item_id)` 唯一；同一媒体条目的多个文件版本共享一条进度，`last_media_file_id` 仅记录最近选中的版本
- 删除最近选择的文件版本不会删除播放进度或继续观看记录；服务端统一选择仍存在的首个版本作为回退，Web、macOS 和 iOS 不需要各自实现不同的版本选择规则
- Web 播放器在播放中按 `5s` 心跳上报，并在暂停、播放结束、切源、切集、页面隐藏和离开页面时强制 flush 一次

### `PUT /api/media-items/{id}/playback-progress`

作用：
- 写入或更新某个媒体条目的播放进度

路径参数：
- `id`：`media_item_id`

请求体：

```json
{
  "media_file_id": 12,
  "position_seconds": 368,
  "duration_seconds": 5400,
  "is_finished": false
}
```

字段说明：
- `media_file_id`：具体播放的文件 ID
- `position_seconds`：当前播放到第几秒
- `duration_seconds`：总时长，可选
- `is_finished`：是否已看完，可选，不传默认为 `false`

关键校验：
- `media_item_id` 必须存在
- `media_file_id` 必须存在
- 该 `media_file_id` 必须属于 URL 里的 `media_item_id`
- `position_seconds` 和 `duration_seconds` 不能为负
- 如果 `position_seconds > duration_seconds`，后端会压到时长上限

返回：
- `200 OK`
- 返回更新后的 `PlaybackProgressResponse`

说明：
- 播放进度按当前登录用户隔离；不同用户的观看记录、继续观看列表互不共享
- `playback_progress` 按用户与媒体条目唯一，只保留“当前最新状态”，不承担完整历史时间线
- 同一媒体条目的不同 `media_file_id` 是资源版本，不会产生多条独立进度；每次上报覆盖这条唯一记录，返回的 `last_media_file_id` 用于恢复最近选择或服务端回退后的有效版本
- 扫描或人工匹配合并重复媒体条目时，媒体文件重归属、播放进度和继续观看状态在同一事务内迁移；同一用户存在两份状态时以 `last_watched_at` 较新的记录为准
- 当 `is_finished = false` 时，服务端会把电影或所属 Series upsert 到 `continue_watching`；同系列切换集数只更新原行
- 当 `is_finished = true` 时，播放进度和完成状态仍保留，但电影或所属 Series 会从 `continue_watching` 删除
- `continue_watching` 每个用户最多保留 20 部唯一电影或 Series，超过上限时服务端删除最旧记录
- 客户端在用户开始播放时应立即上报一次，即使当前位置为 `0`，这样刚选中的电影或剧集会立即进入 Continue

### `GET /api/playback-progress/continue-watching`

作用：
- 查询“继续观看”列表

查询参数：
- `limit`：可选，返回条目数量上限

示例：
- `/api/playback-progress/continue-watching`
- `/api/playback-progress/continue-watching?limit=12`

返回：
- `200 OK`
- 返回 `ContinueWatchingItemResponse[]`

返回结构：

```json
[
  {
    "media_item": {
      "id": 5,
      "library_id": 1,
      "media_type": "movie",
      "title": "The Matrix",
      "source_title": "The Matrix",
      "original_title": null,
      "sort_title": null,
      "metadata_provider": "tmdb",
      "metadata_provider_item_id": "603",
      "metadata_status": "matched",
      "metadata_failure_reason": null,
      "remote_media_type": "movie",
      "year": 1999,
      "ratings": [
        {
          "source": "tmdb",
          "kind": "user",
          "retrieved_via": "provider",
          "score": 8.2,
          "scale": 10.0,
          "rating_count": 27111,
          "attributes": {},
          "fetched_at": "..."
        }
      ],
      "country": "United States",
      "genres": "Action · Science Fiction",
      "studio": "Warner Bros. Pictures",
      "overview": null,
      "poster_path": "/api/media-items/5/poster",
      "backdrop_path": "/api/media-items/5/backdrop",
      "logo_path": "/api/media-items/5/logo",
      "created_at": "...",
      "updated_at": "..."
    },
    "playback_progress": {
      "id": 3,
      "media_item_id": 5,
      "last_media_file_id": 5,
      "position_seconds": 368,
      "duration_seconds": 5400,
      "last_watched_at": "...",
      "is_finished": false
    },
    "season_number": null,
    "episode_number": null,
    "episode_title": null,
    "episode_overview": null,
    "episode_poster_path": null,
    "episode_backdrop_path": null
  }
]
```

说明：
- 只返回 `is_finished = false` 的未看完内容
- 数据来自有上限的 `continue_watching` 活跃队列表，并按最近播放时间倒序返回
- 电影按 `media_item` 聚合；剧集会按 `series` 聚合
- 同一部剧无论看了哪一季哪一集，都只保留最近观看的那一集
- 如果条目来自剧集，`season_number` / `episode_number` / `episode_title` 会标识最近观看的具体集数
- 如果条目来自剧集，`episode_overview` / `episode_poster_path` / `episode_backdrop_path` 会返回最近观看那一集自身的描述和图片；缺失字段保持为空，不会回退到剧集图、季图或另一个集图片字段
- 默认返回 `20` 条，最大 `20` 条

## 8. 媒体流

### `GET /api/media-files/{id}/audio-tracks`

作用：
- 查询某个媒体文件下当前可切换的内嵌音轨列表

路径参数：
- `id`：`media_file_id`

返回：
- `200 OK`
- 返回 `AudioTrackResponse[]`

关键字段：
- `stream_index`：原始媒体文件里的音轨流索引
- `language`：语言代码，例如 `zh`、`en`
- `audio_codec`：音频编码，例如 `aac`、`ac3`
- `label`：音轨标题，例如 `Mandarin Stereo`
- `channel_layout`：声道布局，例如 `stereo`、`5.1(side)`
- `channels`：声道数，例如 `2`、`6`
- `bitrate`：音轨码率，单位 bps
- `sample_rate`：采样率，单位 Hz
- `is_default`：是否是原始文件里的默认音轨

说明：
- 仅列出扫描时通过 `ffprobe` 发现的内嵌音轨
- `source_kind = strm` 的媒体文件没有内嵌音轨清单；客户端不应为其请求或展示内嵌音轨切换。若仍在 STRM 播放请求中传入 `audio_track_id`，服务端返回 `400 strm_audio_track_selection_unsupported`
- 外挂音轨暂不在 MVP 范围内
- 前端通常会额外提供一个 `Auto` 选项，表示不传 `audio_track_id`，直接使用原始文件默认音轨
- 详情页会把音轨列表收成一张音频技术卡，并通过卡头小下拉切换不同轨道

### `GET /api/media-files/{id}/subtitles`

作用：
- 查询某个媒体文件下当前可切换的字幕轨道列表

路径参数：
- `id`：`media_file_id`

返回：
- `200 OK`
- 返回 `SubtitleFileResponse[]`

关键字段：
- `source_kind`：字幕来源，`external` 表示外挂字幕，`embedded` 表示媒体内嵌字幕
- `language`：语言代码，例如 `zh-CN`、`en`
- `subtitle_format`：原始字幕格式，例如 `srt`、`ass`、`ssa`、`vtt`
- `label`：字幕标题或文件名尾部解析出的补充标记
- `is_default`：是否默认字幕
- `is_forced`：是否强制字幕
- `is_hearing_impaired`：是否是听障字幕（例如 `SDH` / `CC` / `HI`）

说明补充：
- 详情页客户端将 `/files`、`/audio-tracks`、`/subtitles` 三组数据组合成视频卡、音轨卡和字幕卡
- 音轨卡和字幕卡通过卡头下拉菜单切换展示的轨道或字幕，不应将所有轨道同时展示为多张卡片

说明：
- 服务端会把外挂字幕和内嵌字幕统一列在这里，前端播放器只需要渲染一份字幕菜单
- STRM 只支持本地外挂字幕，不发现或抽取远端媒体中的内嵌字幕
- 外挂字幕支持：
  - 同目录、同 stem 自动匹配
  - 同目录、季集号一致且目录内唯一时自动匹配，例如 `show.S01E01.mkv` 可匹配 `xxxxx.S01E01.srt`
- 外挂字幕文件名如果命中 `sdh`、`cc`、`hi` 这类后缀，会被标成 `is_hearing_impaired = true`
- 如果同目录下同一个 `SxxEyy` 存在多个视频版本，服务端不会只靠季集号盲猜绑定
- 如果字幕列表查询失败，客户端应当按“字幕暂不可用”降级，主视频播放不应被阻断

### `GET /api/subtitle-files/{id}/stream`

作用：
- 把单条字幕轨道统一转换成浏览器可直接挂载的 `WebVTT`

路径参数：
- `id`：`subtitle_file_id`

返回：
- `200 OK`
- `Content-Type: text/vtt; charset=utf-8`
- 响应体为字幕文本，不是 JSON
- 字幕源文件或转换结果超过处理上限时返回 `413 Payload Too Large` JSON 错误 envelope，`error_code = subtitle_too_large`，`params.max_bytes` 表示本次适用的字节上限

说明：
- `srt` 会在服务端直接转换成 `WebVTT`
- `ass/ssa` 会借助 `ffmpeg` 转成 `WebVTT`
- 内嵌字幕会按流索引抽取后再转成 `WebVTT`
- 外部字幕源最大 16 MiB，转换后的 WebVTT 最大 24 MiB；服务端最多同时执行 4 个字幕生成任务
- 外部字幕和其所属媒体文件在读取前都会解析真实路径，并且必须仍位于关联媒体库根目录内
- 前端播放器切换字幕时，应只激活一条字幕轨道，避免外挂和内嵌字幕同时显示造成重影
- 如果单条字幕流转换或加载失败，客户端应提示该字幕不可用并继续播放主视频，而不是把整个播放器判成失败

### `HEAD /api/subtitle-files/{id}/stream`

作用：
- 只读探测单条字幕轨道的 WebVTT 缓存状态，不生成或转换字幕

路径参数：
- `id`：`subtitle_file_id`

返回：
- 始终为 `200 OK`，没有响应体
- 始终包含 `Content-Type: text/vtt; charset=utf-8`
- 已有有效 WebVTT 缓存时，返回准确的 `Content-Length` 和 `Cache-Control: private, max-age=3600`
- 缓存未命中时，返回 `Cache-Control: no-store`，不返回 `Content-Length`

说明：
- 服务端仍会完成登录鉴权、字幕与媒体文件关联检查，以及媒体文件和外挂字幕的媒体库路径边界校验
- 缓存未命中时不创建缓存目录、不读取字幕内容、不占用字幕生成名额，也不启动 FFmpeg
- 客户端不能把没有 `Content-Length` 解释为长度为零；真正的 WebVTT 会在后续 `GET` 时按需生成

### `GET /api/media-files/{id}/stream`

作用：
- 输出媒体文件流，供浏览器或播放器播放

路径参数：
- `id`：`media_file_id`

可选查询参数：
- `audio_track_id`：指定后端应该优先输出哪条内嵌音轨的 remux 变体

可选请求头：
- `Range: bytes=0-1023`

典型场景：
- `<video src="...">` 直接播放
- 浏览器拖动进度条时的分段读取
- 用户在播放器里切换到另一条内嵌音轨

返回：
- 不带 `Range` 时通常为 `200 OK`
- 带 `Range` 且播放源能够满足该区间时为 `206 Partial Content`
- 响应体是文件流，不是 JSON

关键响应头：
- `Accept-Ranges: bytes`（本地文件或 STRM 上游明确支持字节范围时）
- `Content-Type`
- `Content-Length`
- `Content-Range`（分段请求时）

说明：
- 播放器直接使用这个 URL
- 本地文件和 HTTP(S) STRM 使用同一个 Mova URL；客户端不读取 `.strm`，也不会收到远端 URL
- 不建议前端先 `fetch` 完整文件再转 `blob`
- 本地文件带上 `audio_track_id` 时，服务端会先验证这条音轨确实属于当前媒体文件，再按 `ffmpeg -c copy` 生成缓存变体；这里是 remux，不是转码
- STRM 播放时，服务端重新读取并校验本地载体，在完成用户与媒体库权限校验、DNS/IP 安全检查和重定向逐跳检查后，以有界字节流代理 HTTP(S) 上游；不缓存完整媒体，也不启用 FFmpeg 网络能力
- STRM 代理仅转发单段 `Range` 与 `If-Range`，不转发客户端 Cookie、Authorization、代理凭据或 `X-Forwarded-*`；响应只保留媒体播放所需的安全头，并设置 `Cache-Control: private, no-store` 与 `X-Content-Type-Options: nosniff`
- STRM 仅接受直接媒体响应；HTML、JSON、XML、HLS/MPEGURL、上游错误正文和不一致的 Range 响应不会转发给客户端
- STRM 全局最多 64 条代理流、同一用户最多 4 条；名额用尽立即返回稳定错误，不建立无界等待队列
- STRM 上游忽略从 0 开始的 Range 时可以返回 `200 OK`，但服务端不宣称该响应支持拖动；没有 `If-Range` 时，上游忽略非零 Range 会返回 `416 remote_range_not_supported`。带 `If-Range` 的条件请求失效后，上游可以按 HTTP 语义返回完整 `200 OK`
- 已有有效缓存时直接返回，不进入 remux 排队。缓存未命中且进程内 2 个 remux 名额已经占满时立即返回 `503 service_unavailable`；同一缓存键最多等待 5 秒，超时同样返回 `503`，客户端可以稍后重试
- 生成前同时检查缓存总配额和缓存卷实际可用空间；任何新任务都必须为在途任务预留空间，并在生成后至少保留 5 GiB 可用空间
- remux 变体只服务于源码直放，不提供多码率或自适应码流

### `HEAD /api/media-files/{id}/stream`

作用：
- 返回媒体流相关响应头，不返回实体内容

路径参数：
- `id`：`media_file_id`

可选查询参数：
- `audio_track_id`

可选请求头：
- `Range`

典型场景：
- 浏览器或播放器探测资源头信息

返回：
- 原媒体或已有音轨缓存变体返回 `200 OK` 或 `206 Partial Content`
- 指定 `audio_track_id` 但缓存未命中时返回 `200 OK`
- 没有响应体

说明：
- 前端通常不需要手动调用
- 浏览器播放器可能会自己使用
- STRM 优先向上游发送 `HEAD`；上游返回 `405` 或 `501` 时，服务端改用 `GET Range: bytes=0-0` 探测并立即释放正文
- 本地文件请求 `audio_track_id` 时仍会校验音轨归属，但 `HEAD` 不启动 FFmpeg，也不等待生成任务；STRM 请求携带该参数时返回 `400 strm_audio_track_selection_unsupported`
- 已有有效音轨缓存时返回该变体准确的 `Accept-Ranges`、`Content-Length` 和可选 `Content-Range`
- 音轨缓存未命中时只返回确定的 `Content-Type`、`Cache-Control: no-store` 和 `Accept-Ranges: none`；不返回原媒体文件的 `Content-Length` 或 `Content-Range`，避免把源文件长度误报为音轨变体长度
- 客户端不能把没有 `Content-Length` 解释为长度为零；真正的音轨变体在后续 `GET` 时按需生成

#### STRM 播放错误

STRM 播放错误仍使用通用 JSON 错误 envelope；`message` 只作诊断兜底，客户端必须优先按 `error_code + params` 本地化：

| HTTP | `error_code` | 客户端语义 |
|---|---|---|
| `400` | `strm_audio_track_selection_unsupported` | 远程流不支持指定内嵌音轨 |
| `403` | `strm_target_forbidden` | URL、端口、DNS 或目标地址被安全策略拒绝 |
| `413` | `strm_reference_too_large` | 本地引用文件超过读取上限 |
| `416` | `remote_range_not_supported` | 上游不能满足非零 Range |
| `422` | `strm_reference_invalid` | 当前引用内容已经无效 |
| `429` | `strm_user_stream_limit_exceeded` | 当前用户的远程流并发数达到上限 |
| `502` | `remote_source_unavailable` | DNS、连接、上游状态或远端资源不可用 |
| `502` | `remote_response_invalid` | 上游内容类型或 Range 响应不符合直接媒体要求 |
| `503` | `strm_stream_capacity_exhausted` | 服务端全局远程流名额已满 |
| `504` | `remote_source_timeout` | DNS、连接或响应头超时 |

上游响应体开始传输后若连接中断，HTTP 状态已经不能改写；客户端按普通媒体流中断处理。服务端日志不得记录原始 URL、查询参数或上游错误正文。

## 9. ID 关系说明

客户端需要区分以下三个 ID：

- `library_id`
  - 来自 `/api/libraries` 或 `/api/libraries/{id}`
  - 用于媒体库相关接口

- `media_item_id`
  - 来自 `/api/libraries/{id}/media-items`
  - 用于媒体条目详情、文件列表、播放进度

- `media_file_id`
  - 来自 `/api/media-items/{id}/files`
  - 用于媒体流播放和播放进度上报

- `audio_track_id`
  - 来自 `/api/media-files/{id}/audio-tracks`
  - 用于播放器切换内嵌音轨

- `subtitle_file_id`
  - 来自 `/api/media-files/{id}/subtitles`
  - 用于播放器加载单条字幕轨道内容

推荐前端流转：

1. 调 `GET /api/libraries/{library_id}/media-items`
2. 取某条记录的 `media_item_id`
3. 调 `GET /api/media-items/{media_item_id}/files`
4. 取文件列表中的 `media_file_id`
5. 如需音轨菜单，先调 `GET /api/media-files/{media_file_id}/audio-tracks`
6. 如需字幕菜单，再调 `GET /api/media-files/{media_file_id}/subtitles`
7. 选中字轨后，用 `subtitle_file_id` 请求 `/api/subtitle-files/{subtitle_file_id}/stream`
8. 播放时：
   - 默认音轨：`<video src="/api/media-files/{media_file_id}/stream" />`
   - 切换音轨后：`<video src="/api/media-files/{media_file_id}/stream?audio_track_id={audio_track_id}" />`
   - `PUT /api/media-items/{media_item_id}/playback-progress`
