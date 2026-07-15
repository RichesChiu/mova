# Mova API 一览

这份文档整理当前 `mova-server` 已实现的全部 HTTP 接口，重点说明每个接口的用途、关键入参和返回语义。

## 通用说明

- Base URL：默认 `http://127.0.0.1:36080`
- 响应格式：
  - 普通业务接口默认返回 JSON，并统一包裹成 `code / message / data`
  - 媒体流和图片资源接口返回文件流，不返回 JSON
- 鉴权：
  - `GET /api/health`、`GET /api/auth/bootstrap-status`、`POST /api/auth/bootstrap-admin`、`POST /api/auth/login`、`POST /api/auth/token-login`、`POST /api/auth/refresh` 可匿名访问
  - 其他接口都要求登录态
  - Web 端继续使用 session cookie
  - 原生客户端使用 `Authorization: Bearer <access_token>` 访问业务接口，`access_token` 和 `refresh_token` 通过 `POST /api/auth/token-login` 获取；`refresh_token` 只能调用 `POST /api/auth/refresh`，不能访问普通业务接口
  - 管理类接口（用户管理、建库、删库、触发扫描、服务器根目录等）要求 `admin`
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
  "message": "resource not found",
  "data": null
}
```

认证相关错误会使用字符串 `code`，例如：

```json
{
  "code": "TOKEN_EXPIRED",
  "message": "Access token expired",
  "data": null
}
```

- 文档中后续字段示例多数只展示 `data` 内部结构，真实响应会额外包一层统一 envelope。

- 常见状态码：
  - `200 OK`：请求成功
  - `201 Created`：创建成功
  - `202 Accepted`：异步任务已创建并开始后台执行
  - `400 Bad Request`：请求参数或业务校验不通过
  - `401 Unauthorized`：未登录、access token 无效/过期，或 refresh token 无效/过期/已撤销
  - `403 Forbidden`：已登录但没有权限访问
  - `409 Conflict`：资源当前状态不允许执行该操作
  - `404 Not Found`：资源不存在
  - `416 Range Not Satisfiable`：媒体流的 `Range` 请求越界
  - `500 Internal Server Error`：服务内部错误
- TMDB provider 现在从运行时环境变量 `MOVA_TMDB_ACCESS_TOKEN` 读取；但每个媒体库仍可单独配置 `metadata_language`，决定扫描与元数据补全时使用 `zh-CN` 或 `en-US`。
- 如果额外配置了可选的 `MOVA_OMDB_API_KEY`，服务端会在已拿到 `imdb_id` 的前提下补齐 `imdb_rating`；不配置时该字段保持为空，不影响扫描、入库和播放。
- 本地海报和背景图这类图片资源，对外返回的 URL 现在会带版本参数（例如 `/api/media-items/42/poster?v=1704164645`），浏览器可以长期缓存；当媒体元数据更新时，版本参数会变化，前端会自动拿到新图。
- 当前 pre-1.0 阶段的数据库 schema 只维护 `migrations/0001_init.sql`；本版已删除 `libraries.is_enabled`，会保存扫描本地分析版本、原生客户端 access/refresh token 设备会话，并用 `playback_progress` 保存逐文件进度、用有上限的 `continue_watching` 保存当前活跃队列。后台扫描通过 PostgreSQL `background_jobs` 持久化，资源版本通过 `realtime_revisions` 持久化；TMDB / provider 返回的标题、国家、题材、制作公司、演员角色等自由文本字段按 `text` 存储。已有开发库需要重建数据库 / 重置数据目录并重新扫描媒体库，旧 migration 不会被平滑升级。

## 接口总览

| Method | Path | 作用 |
| --- | --- | --- |
| `GET` | `/api/health` | 健康检查 |
| `GET` | `/api/auth/bootstrap-status` | 查询是否需要初始化首个管理员 |
| `POST` | `/api/auth/bootstrap-admin` | 初始化首个管理员并登录 |
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
| `PATCH` | `/api/users/{id}` | 更新用户基础信息（管理员） |
| `DELETE` | `/api/users/{id}` | 删除用户（管理员） |
| `PUT` | `/api/users/{id}/password` | 管理员重置指定用户密码 |
| `GET` | `/api/server/media-tree` | 查询服务端当前可用于建库的媒体文件夹树 |
| `GET` | `/api/libraries` | 查询媒体库列表 |
| `GET` | `/api/libraries/recently-added` | 查询按库分组的最新添加内容 |
| `POST` | `/api/libraries` | 创建媒体库 |
| `GET` | `/api/libraries/{id}` | 查询单个媒体库详情 |
| `PATCH` | `/api/libraries/{id}` | 更新媒体库基础配置 |
| `DELETE` | `/api/libraries/{id}` | 删除媒体库 |
| `GET` | `/api/libraries/{id}/media-items` | 查询媒体库下的媒体条目列表 |
| `GET` | `/api/libraries/{id}/scan-jobs` | 查询媒体库扫描历史 |
| `GET` | `/api/libraries/{id}/scan-jobs/{scan_job_id}` | 查询单个扫描任务状态 |
| `POST` | `/api/libraries/{id}/scan` | 触发异步扫描 |
| `GET` | `/api/search` | 搜索当前用户可见库下的电影、剧集和集条目 |
| `GET` | `/api/media-items/{id}` | 查询单个媒体条目详情 |
| `GET` | `/api/media-items/{id}/cast` | 查询单个媒体条目的演员列表 |
| `GET` | `/api/media-items/{id}/playback-header` | 查询播放器页头部信息 |
| `GET` | `/api/media-items/{id}/files` | 查询媒体条目关联文件列表 |
| `GET` | `/api/media-items/{id}/episode-outline` | 查询剧集全集大纲并标记本地可用集 |
| `GET` | `/api/media-items/{id}/metadata-search` | 手动搜索单条媒体的候选元数据（管理员） |
| `POST` | `/api/media-items/{id}/metadata-match` | 选择候选结果并替换当前媒体元数据（管理员） |
| `POST` | `/api/media-items/{id}/refresh-metadata` | 手动重拉单个媒体条目元数据 |
| `GET` | `/api/media-items/{id}/poster` | 读取媒体条目海报图 |
| `GET` | `/api/media-items/{id}/backdrop` | 读取媒体条目背景图 |
| `GET` | `/api/seasons/{id}/poster` | 读取某一季海报图 |
| `GET` | `/api/seasons/{id}/backdrop` | 读取某一季背景图 |
| `GET` | `/api/media-items/{id}/playback-progress` | 查询单条内容的最近播放进度 |
| `PUT` | `/api/media-items/{id}/playback-progress` | 写入或更新播放进度 |
| `GET` | `/api/playback-progress/continue-watching` | 查询继续观看列表 |
| `GET` | `/api/media-files/{id}/audio-tracks` | 查询媒体文件可切换的内嵌音轨列表 |
| `GET` | `/api/media-files/{id}/subtitles` | 查询媒体文件可切换字幕列表 |
| `GET` | `/api/media-files/{id}/stream` | 播放媒体文件 |
| `HEAD` | `/api/media-files/{id}/stream` | 查询媒体文件播放头信息 |
| `GET` | `/api/subtitle-files/{id}/stream` | 输出单条字幕轨道的 WebVTT 内容 |

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
  "status": "ok"
}
```

## 2. 认证与用户

所有认证和用户管理接口继续使用 `username` 作为请求字段名，界面将它展示为登录账户。服务端会去除首尾空白，并限制为 1–254 个字符，因此可以使用普通账号名或邮箱形式的登录标识；邮箱形式只作为精确匹配的账户字符串，不代表 Mova 会校验邮箱归属或发送邮件。已有 pre-1.0 数据库需要重建 `data/postgres/`，才能把底层 `users.username` 字段从 64 个字符扩展到 254 个字符。

### `GET /api/auth/bootstrap-status`

作用：
- 查询当前系统是否还没有管理员，前端可据此决定显示“初始化首个管理员”还是普通登录页

返回：
- `200 OK`

```json
{
  "bootstrap_required": true
}
```

### `POST /api/auth/bootstrap-admin`

作用：
- 仅在系统还没有管理员时，创建第一个 `admin` 用户并直接建立登录态

请求体：

```json
{
  "username": "admin",
  "password": "admin123456"
}
```

说明：
- 一旦系统里已经存在管理员，再调用会返回 `409 Conflict`
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
    "role": "admin",
    "is_primary_admin": true,
    "is_enabled": true,
    "library_ids": []
  }
}
```

说明：
- `access_token` 当前默认有效期 2 小时，只用于访问普通业务接口
- `refresh_token` 当前默认有效期 30 天，只用于调用 `POST /api/auth/refresh`
- 服务端只保存 token hash，不明文保存原始 token
- 后续业务请求把 `Authorization: Bearer <access_token>` 带到受保护接口即可
- access token 过期、refresh token 过期/撤销、用户被禁用/删除/改密后，对应原生客户端会话会失效
- Web 端不需要使用这个接口，仍然继续调用 `POST /api/auth/login`

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
    "role": "admin",
    "is_primary_admin": true,
    "is_enabled": true,
    "library_ids": []
  }
}
```

说明：
- refresh 成功后旧 `refresh_token` 会立即失效
- 旧 `refresh_token` 被重复使用时，服务端会视为异常重放并撤销对应原生客户端设备会话
- 用户被禁用、删除或改密后，旧 `access_token` 和 `refresh_token` 都不能继续使用
- 失败时常见错误码包括 `INVALID_REFRESH_TOKEN`、`REFRESH_TOKEN_EXPIRED`、`SESSION_REVOKED`

### `POST /api/auth/logout`

作用：
- 删除当前登录态对应的服务端会话记录；如果当前是 cookie 登录，还会顺带清理 session cookie

可选请求体：

```json
{
  "refresh_token": "long-lived-refresh-token"
}
```

返回：
- `200 OK`

说明：
- 支持 cookie、Bearer access token 和请求体里的 `refresh_token`
- 如果同时带了 cookie 和 `Authorization`，服务端会优先使用 Bearer token
- 原生客户端应尽量在登出时同时提交当前 `refresh_token`；如果 access token 已过期但 refresh token 仍有效，服务端仍会撤销对应设备会话

### `GET /api/auth/me`

作用：
- 查询当前登录用户

返回：
- `200 OK`
- 返回字段包括 `id`、`username`、`nickname`、`role`、`is_primary_admin`、`is_enabled`、`library_ids`
- 支持 cookie 和 Bearer access token 两种登录态；不接受 refresh token
- `is_primary_admin = true` 只会出现在系统初始化出来的首个管理员身上；它可以创建、提升、编辑和删除普通管理员

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
  - `protocol_version`：当前 Realtime/SSE 协议版本，当前值为 `1`。

说明：
- 进入具体媒体库后再使用 `GET /api/libraries/{id}/media-items` 分页加载完整目录。
- 客户端可以把 `realtime.resources` 作为当前**首页读模型**的 revision 基线，避免紧接着收到重复失效通知后再次刷新首页；它不能替代媒体详情、用户管理等独立读模型的首次加载或失效处理。

### Realtime / SSE 协议

Realtime 的资源 revision、事件触发条件、完整 payload、跨 Web/macOS/iOS 客户端状态机、断线恢复和当前架构边界统一见 [`REALTIME.md`](REALTIME.md)。下面只保留接口级摘要。

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
- `server_epoch` 在当前数据库生命周期内保持稳定；数据库重建后会变化。客户端发现 epoch 变化时应丢弃本地 revision 基线并重新同步。
- `resources` 只包含当前用户有权访问的资源；尚未变化过的资源 revision 为 `0`。
- `active_scans` 返回当前仍为 `pending` 或 `running` 的扫描任务。扫描 `phase` 会持久化，不依赖 SSE 临时状态恢复。

### `GET /api/realtime/events`

作用：
- 订阅资源失效通知与临时扫描进度。SSE 不承载最终业务数据，也不保证客户端收到每一条临时进度。

说明：
- 需要登录态，支持 cookie 和 Bearer access token。
- 返回类型为 `text/event-stream`，服务端每 15 秒发送 keep-alive。
- 服务端只推送连接建立之后的新事件，不回放历史；客户端重连后必须先调用 `GET /api/realtime/state` 做 revision 差异同步。
- 资源变更由数据库事务同步增加 `realtime_revisions`，即使 SSE 丢失或服务重启，revision 仍可恢复。
- 普通资源最多每 500ms 合并一批；继续观看默认最多每 1 秒合并一批，标记已看完会立即通知。
- 扫描进度按 `(scan_job_id, item_key)` latest-wins 合并，最多每 200ms 发送一批；普通进度在 Dispatcher 饱和时允许丢弃，`scan.finished` 作为终态事件会等待队列容量并立即发送。
- SSE 最后一跳按 server/admin/library/user scope 使用独立有界队列。连接只订阅与自己有关的 scope，无关用户或媒体库的高频事件不会唤醒该连接。客户端在相关队列中明显落后时，服务端发送一次 `resync.required` 后关闭连接，客户端应重新获取 state 并重连。
- 权限变化或会话撤销会发送 `session.invalidated` 并关闭当前连接。

#### `resources.changed`

```text
event: resources.changed
data: {"protocol_version":1,"changes":[{"resource":"library:7:catalog","revision":128}]}
```

客户端只在服务端 revision 大于本地已应用 revision 时刷新对应资源；重复事件和乱序旧事件应忽略。当前资源键包括：
- `admin:libraries`
- `library:{id}:settings`
- `library:{id}:catalog`
- `library:{id}:scan`
- `user:{id}:libraries`
- `user:{id}:profile`
- `user:{id}:continue-watching`
- `admin:users`

#### `scan.progress` / `scan.finished`

```text
event: scan.progress
data: {
  "protocol_version": 1,
  "scan_job": {
    "id": 41,
    "library_id": 7,
    "status": "running",
    "phase": "enriching",
    "total_files": 240,
    "scanned_files": 52
  },
  "items": [
    {
      "scan_job_id": 41,
      "library_id": 7,
      "item_key": "series-title:arcane",
      "media_type": "series",
      "title": "Arcane",
      "item_index": 52,
      "total_items": 240,
      "stage": "artwork",
      "progress_percent": 76
    }
  ]
}
```

- `scan.progress` 是可丢失的临时 UI 状态，同一 `item_key` 只保留最新值。
- `scan.finished` 在相同任务和条目字段之外增加 `changes`，其中包含当前可读取到的 `library:{id}:catalog` 与 `library:{id}:scan` revision。客户端把这些 change 交给统一 Revision Coordinator，刷新成功后再移除临时扫描卡片；不再为终态事件单独实现一套 catalog 刷新路径。
- 扫描 phase 使用 `initializing` / `discovering` / `analyzing` / `enriching` / `syncing` / `finished`。
- 条目 stage 使用 `discovered` / `metadata` / `artwork` / `completed`。

#### 恢复与会话事件

```text
event: resync.required
data: {"protocol_version":1,"reason":"client_lagged"}

event: session.invalidated
data: {"protocol_version":1,"reason":"authorization_changed"}
```

- 收到 `resync.required` 后重新获取 realtime state，只刷新 revision 不一致的资源。
- 收到 `session.invalidated` 后停止实时连接并重新建立登录态。

更完整的事件使用规则和客户端验收清单见 [`REALTIME.md`](REALTIME.md)。

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
- 修改成功后会轮换 session，旧会话失效，响应会写回新的 session cookie
- 修改成功后，该用户现有原生客户端 access/refresh token 也会全部撤销；原生客户端应使用新密码重新调用 `POST /api/auth/token-login`

### `GET /api/users`

作用：
- 管理员查看当前所有用户

说明：
- `admin` 用户的 `library_ids` 始终为空数组，语义上表示“默认拥有全部媒体库访问权”
- `viewer` 用户的 `library_ids` 表示允许访问的媒体库 ID 列表
- `is_primary_admin = true` 的管理员表示当前系统的主管理员；普通管理员仍然拥有媒体库管理能力，但不能管理平级管理员，也不能管理主管理员

### `POST /api/users`

作用：
- 管理员创建一个新用户

请求体：

```json
{
  "username": "viewer01",
  "nickname": "Cinema Fan",
  "password": "viewer1234",
  "role": "viewer",
  "is_enabled": true,
  "library_ids": [1, 2]
}
```

字段说明：
- `username`：用于登录的账户；服务端会去除首尾空白，长度必须为 1–254 个字符，可使用普通账号名或邮箱形式的精确匹配字符串
- `role`：只支持 `admin` / `viewer`
- `library_ids`：只对 `viewer` 生效；`admin` 会忽略这个字段

权限约束：
- 只有主管理员可以创建新的 `admin`
- 普通管理员只能创建 `viewer`

### `PATCH /api/users/{id}`

作用：
- 管理员更新用户基础信息

请求体：

```json
{
  "username": "viewer01",
  "nickname": "Cinema Fan",
  "role": "viewer",
  "is_enabled": true,
  "library_ids": [1, 2]
}
```

字段说明：
- 所有字段都可选，不传表示保持原值
- `library_ids` 是更新 `viewer` 媒体库访问范围的唯一字段；传入数组会整体替换原授权，不传则保持原值
- `library_ids` 只对 `viewer` 生效；更新为 `admin` 时会自动清空库授权

关键约束：
- 当前用户不能通过该接口禁用自己
- 当前用户不能通过该接口修改自己的角色
- 不能降级、禁用最后一个启用中的管理员
- 禁用用户后，服务端会清理该用户现有 Web session 和原生客户端 access/refresh token 会话
- 只有主管理员可以编辑普通管理员
- 主管理员也可以启用或禁用普通管理员
- 普通管理员不能修改或降级其他管理员，也不能修改主管理员

### `DELETE /api/users/{id}`

作用：
- 管理员删除指定用户

说明：
- 当前用户不能删除自己
- 不能删除最后一个启用中的管理员
- 删除后会级联清理该用户的库授权、会话和播放进度
- 只有主管理员可以删除普通管理员
- 主管理员本身不能通过该接口被删除

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
- 重置成功后，该用户现有 Web session 和原生客户端 access/refresh token 会话会全部失效
- 只有主管理员可以重置普通管理员密码

## 3. 服务器媒体目录

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
- 宿主机媒体根目录由服务端配置文件中的 `MOVA_MEDIA_ROOT` 配置，并挂载到容器内 `/media`
- 返回树的根节点 `path` 表示客户端当前可见的服务端根目录
- 服务端递归读取全部子文件夹，并按名称排序
- 客户端不得把本机文件系统路径作为服务端 `root_path`

## 4. 媒体库

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
        "metadata_provider_item_id": 123,
        "metadata_status": "matched",
        "metadata_failure_reason": null,
        "remote_media_type": "series",
        "year": 2023,
        "imdb_rating": "8.6",
        "country": "US",
        "genres": "Drama, Adventure",
        "studio": null,
        "overview": null,
        "poster_path": "/api/media-items/42/poster?v=1780630000",
        "backdrop_path": "/api/media-items/42/backdrop?v=1780630000",
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
- `metadata_language`：TMDB 元数据语言，当前支持 `zh-CN` / `en-US`，不传时默认 `zh-CN`
- `root_path`：要扫描的本地目录

关键校验：
- 名称不能为空
- 路径不能为空
- 路径必须存在且必须是目录

返回：
- 成功时 `201 Created`
- 返回创建后的 `LibraryResponse`

说明：
- 创建媒体库后会自动触发一次后台扫描；后续仍可显式调用 `POST /api/libraries/{id}/scan`
- 媒体库不再提供启用/禁用状态；已创建的库始终可以被手动扫描
- 当前允许重叠或完全相同的 `root_path`。同一个物理文件如果被多个库路径覆盖，会在各自库里独立建模和展示。
- 媒体库现在统一自动识别电影和剧集，不再要求用户选择库类型。扫描时会按单个视频文件判断：
  - 文件名里命中 `剧名.S01E02.mkv`、`剧名 S01E02 - 第 2 集.mkv`、`剧名 - S01E02.mkv`、`剧名_S01E02.mkv`、`剧名-S01E02.mkv`、`剧名.1x02.mkv`、`剧名S01E02.mkv` 这类显式剧名和季集信号时，优先按文件名里的剧名归组
  - 显式剧集文件如果位于明确季目录树下，会先按共同剧集容器聚合，再统一写库；同一剧集目录里不同季、不同语言文件名不会被拆成多个剧集；季目录上一级如果带年份，例如 `流氓读书会 (2025)`，年份只作为远端元数据搜索提示
  - 剧集文件名里的年份只作为元数据匹配提示，不作为剧集身份键；例如 `The Boys (2019) - S01E01.mkv` 和 `The Boys (2020) - S02E01.mkv` 会自动聚合到同一剧集
  - `第 1 集`、`Episode 1` 这类跟在季集号后的通用集数文案不会当作远端集标题，远端集标题仍可在刮削成功后覆盖
  - 文件名只有 `S01E02.mkv`、`01.mkv`、`EP02.mkv`、`第03集.mkv` 这类季集或集号时，不再结合目录信号归组
  - 本地猜测为电影或剧集的扫描组都会同时参考 TMDB movie / tv 搜索结果；当两种远端候选都存在时优先验证本地结构对应的类型，只有对应类型不存在合格候选时才判定类型冲突
  - 本地剧集结构只搜到同名电影、或本地电影猜测只搜到同名剧集时，不跨类型改写；文件会以 `metadata_status = unmatched`、`metadata_failure_reason = remote_type_mismatch` 和实际远端 `remote_media_type` 入库，并进入前端 `Other` 复核区
  - 本地分析完成、远端确认尚未完成时使用 `metadata_status = pending`，前端按本地猜测类型展示；远端完全未命中、类型冲突或检测失败只会在 `stage = completed` 后进入 `Other`
  - 如果没有启用 TMDB，文件完成时会以 `metadata_status = skipped` 入库；这种情况不视为刮削失败，但由于没有远端类型确认，完成后进入 `Other`

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
- `last_scan`：最近一次扫描摘要，没有时为 `null`
- `last_scan.phase`：持久化的最近扫描阶段，当前使用 `initializing` / `discovering` / `analyzing` / `enriching` / `syncing` / `finished`；服务重启后仍可通过 HTTP 恢复

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
- 如果当前库有正在执行的扫描任务，服务会先请求取消并等待它退出，再真正删除库
- 删除库时服务会在同一事务内显式清理该库的扫描任务、授权关系、媒体条目、资源文件、字幕、音轨、季集、演员缓存和播放进度，再删除 `libraries` 记录
- 删除库后，服务会清理该库曾引用且不再被任何剩余媒体、季或演员记录引用的 `MOVA_CACHE_DIR/tmdb` 本地图片缓存；媒体目录里的 sidecar 图片不会被删除
- 如果同一时间重复删除同一个库，或扫描仍在停止过程中，会返回 `409 Conflict`

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
- `metadata_language`：可选，更新 TMDB 元数据语言，当前支持 `zh-CN` / `en-US`

返回：
- 成功时 `200 OK`
- 返回更新后的 `LibraryResponse`

说明：
- 至少要传一个字段，否则返回 `400 Bad Request`
- 只更新名称或描述不会触发扫描
- 当 `metadata_language` 发生变化时，服务端会先停止该库当前正在执行的扫描，把库内所有媒体条目标记为 `metadata_status = pending`，然后自动创建一次覆盖全库的元数据扫描；文件未变化时会复用既有本地分析、音轨和字幕结果，但会按新语言重新请求全部远端元数据
- 媒体库不再提供启用/禁用状态，更新接口也不再接受该字段

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
- `query`：可选，按名称筛选，会匹配 `title` 和 `original_title`
- `year`：可选，按发行年精确筛选

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
- 列表当前返回顶层媒体条目，也就是电影和剧；剧集的单集不会直接出现在这个列表里
- `items[]` 使用 `MediaItemResponse`，会返回 `metadata_status` / `metadata_failure_reason` / `remote_media_type`；`pending` 条目按本地 `media_type` 进入 Movies / Series。完成后只有 `remote_media_type` 为空、或远端类型与本地结构冲突的条目进入 `Other`；远端类型已确认且一致的失败条目仍留在对应类型分区
- 默认按名称升序返回
- 当前只支持名称筛选和发行年筛选，尚未支持更多排序和筛选组合

### `GET /api/libraries/{id}/scan-jobs`

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
- `status`：`pending` / `running` / `success` / `failed`
- `phase`：持久化的当前扫描阶段，使用 `initializing` / `discovering` / `analyzing` / `enriching` / `syncing` / `finished`
- `scanned_files`：当前已发现文件数
- `total_files`：当前已知总文件数
- `error_message`：失败原因；现在会直接带阶段上下文，例如：
  - `扫描目录阶段失败：扫描文件目录失败：无法读取媒体库目录 /media/movies：...`
  - `元数据补全阶段失败：整理媒体条目失败：...`
  - `写入媒体库阶段失败：写入媒体库失败：...`

### `POST /api/libraries/{id}/scan`

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
- 当前库如果已经有 `pending` 或 `running` 任务，不会重复启动第二个扫描
- 扫描请求会和 PostgreSQL `background_jobs` 后台任务在同一事务内持久化；服务重启后 worker 会重新领取未完成任务，不再把所有中断任务直接标记失败。客户端可以通过 `/api/libraries/{id}/scan-jobs/{scan_job_id}` 查询，也可以通过 realtime state 和临时扫描事件展示进度
- 当前扫描会按 `(library_id, file_path)` 做增量同步：同路径文件原地更新，缺失路径删除，改名或移动会表现成旧路径删除加新路径新增
- 已经成功匹配的路径会先按文件大小和修改时间生成稳定指纹；同路径指纹一致、本地分析版本一致、且已有 TMDB 绑定时会跳过拆名、sidecar、`ffprobe`、TMDB / OMDb、图片缓存和数据库 upsert，只保留现有数据。新增、变化或本地分析版本过期的路径会先做浅层文件名聚合，再按扫描组逐个完整探测、写库并推送。`unmatched` / `failed`、缺少 TMDB provider 绑定、旧状态是 `skipped` 但当前已启用 TMDB、按前端 Other 规则需要复核、或仍保存远端图片 URL 的条目仍会在后续手动扫描中重试；如果这些条目的文件指纹和本地分析版本未变化，服务端仍用当前文件名 / 路径做浅层聚合，再复用已入库的本地分析结果，仅重试 TMDB 和图片缓存。电影补全拿到相同 TMDB `provider_item_id` 后会合并为同一个 `media_item`，详情页通过资源版本切换展示多个本地文件。自动候选选择保持保守；更宽松的候选复核交给手动搜索 / 替换元数据。如果当前没有启用 metadata provider，`skipped` 条目也可按指纹和本地分析版本跳过
- 现在只有手动扫描会驱动这套库存对齐与元数据补全链路；新增、删除、改名和移动都会在手动扫描时收敛出来

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

## 5. 媒体条目

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
- `source_title`：文件名解析出的原始资源名，主要用于后续元数据匹配和问题排查，不建议直接作为前端展示名
- `metadata_provider` / `metadata_provider_item_id`：远端 metadata 绑定信息，只表达当前条目是否绑定到具体 TMDB 条目，不再用于判断 `Other`
- `metadata_status`：元数据处理状态，当前会使用 `pending` / `matched` / `unmatched` / `failed` / `skipped`；`pending` 只表示扫描中的远端确认中间态
- `metadata_failure_reason`：当 `metadata_status` 为 `unmatched` 或 `failed` 时解释原因，例如 `no_remote_match`、`remote_type_mismatch`、`remote_detection_failed`、`metadata_provider_error`
- `remote_media_type`：远端识别到的媒体类型，当前会使用 `movie` / `series`；没有远端判断或 TMDB 未启用时可为 `null`
- `imdb_rating`：可选的 IMDb 评分字符串；只有在配置了 `MOVA_OMDB_API_KEY` 且当前条目能解析到 `imdb_id` 时才会有值
- `country`：可选的国家/地区信息；电影会优先使用 TMDB 的 production countries，剧集会优先使用 TMDB 的 origin country；服务端按自由文本存储，不做 255 字符截断
- `genres`：可选的题材类型字符串；来自 TMDB genres，会按展示顺序拼接；服务端按自由文本存储，不做 255 字符截断
- `studio`：可选的制作公司字符串；来自 TMDB production companies，会按展示顺序拼接；服务端按自由文本存储，不做 255 字符截断
- `overview`：简介，可来自本地 sidecar `.nfo` 或 TMDB
- `poster_path`：海报可访问 URL；TMDB 图片会优先缓存到本地，因此通常是 `/api/media-items/{id}/poster`
- `backdrop_path`：背景图可访问 URL；TMDB 图片会优先缓存到本地，因此通常是 `/api/media-items/{id}/backdrop`

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
  "metadata_provider_item_id": 94605,
  "metadata_status": "matched",
  "metadata_failure_reason": null,
  "remote_media_type": "series",
  "year": 2021,
  "imdb_rating": "9.0",
  "country": "US",
  "genres": "Animation · Action & Adventure · Sci-Fi & Fantasy",
  "studio": "Fortiche Production",
  "overview": "……",
  "poster_path": "/api/media-items/3/poster",
  "backdrop_path": "/api/media-items/3/backdrop",
  "created_at": "2026-03-24T12:00:00+08:00",
  "updated_at": "2026-03-24T12:00:00+08:00"
}
```

### `GET /api/media-items/{id}/cast`

作用：
- 查询单个媒体条目的主演员列表
- 服务端会先读取本地已持久化的演员列表
- 如果当前条目还没有演员信息，会在这个请求里按需拉一次远端演员并直接写库
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
    "person_id": 12345,
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
- 如果该条目已经完成 TMDB 元数据增强，这里的标题会优先使用增强后的标题
- 如果当前播放的是剧集，且当前集和所在季都还没有片头区间，服务端会在返回头部信息前按需触发一次 season 级片头检测；检测失败不会阻断播放，只是这次仍按“无片头数据”处理

返回示例：

```json
{
  "media_item_id": 42,
  "library_id": 1,
  "media_type": "episode",
  "title": "Severance",
  "original_title": "Severance",
  "year": 2022,
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
- `file_path`：后端内部文件路径
- `container`：容器格式，如 `mp4` / `mkv`
- `duration_seconds` / `video_codec` / `audio_codec` / `width` / `height` / `bitrate`：基础探测字段
- `video_title` / `video_profile` / `video_level`：视频流标题、profile、level
- `video_bitrate` / `video_frame_rate` / `video_aspect_ratio` / `video_scan_type`：视频码率、帧率、宽高比、扫描类型
- `video_color_primaries` / `video_color_space` / `video_color_transfer`：色彩原色、色域、传递特性
- `video_bit_depth` / `video_pixel_format` / `video_reference_frames`：位深、像素格式、参考帧
- `technical_tags`：从 `ffprobe` 探测结果归一化出来的资源技术标签，例如 `HDR10`、`HDR10+`、`Dolby Vision`、`HLG`、`DTS`、`DTS-HD`、`Atmos`

说明：
- 当前前端播放时应先从这个接口拿到 `media_file_id`
- 如果服务运行环境里安装了 `ffprobe`，扫描时会尽量填充时长、编码、分辨率、码率和 `technical_tags`
- `technical_tags` 是文件维度字段；同一个电影或单集有多个版本时，每个 `media_file` 可以返回不同标签
- 如果没有安装 `ffprobe`，或者文件探测失败，这些字段会保持为空，但不会阻断扫描
- 如果这个条目是 `series`，这里通常返回空列表；季集层级和本地可用性统一改用 `/api/media-items/{id}/episode-outline`

### `GET /api/media-items/{id}/episode-outline`

作用：
- 查询剧集“全集大纲 + 本地可用性”
- 这是客户端读取季、集层级的唯一接口，替代分开查询季列表和单季集列表的旧设计

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
  - `seasons[].episodes[].playback_progress.position_seconds`
  - `seasons[].episodes[].playback_progress.duration_seconds`
  - `seasons[].episodes[].playback_progress.last_watched_at`
  - `seasons[].episodes[].playback_progress.is_finished`

说明：
- 当前会优先尝试 TMDB 剧集大纲，并与本地已入库集进行合并。
- 返回结果只包含“至少有一集本地资源”的季；纯远端季不会出现在 `seasons[]` 中。
- TMDB 不可用或匹配失败时，会退化为仅返回本地已入库集。
- TMDB 侧目前可直接提供季海报（`season poster`）和集剧照（`episode still`）；剧集详情大纲里的季只返回 `poster_path`，整页背景应使用剧集条目自身的 `backdrop_path`，集剧照只写入集级 `poster_path`，不会再同时复制到 `backdrop_path`。
- 若集级图片缺失，后端保持为空；不会尝试从本地视频抽取第一帧回退，也不会把通用目录海报（如 `poster.jpg` / `folder.jpg`）、季图或剧图误当成单集封面。
- `seasons[].intro_start_seconds` / `seasons[].intro_end_seconds` 当前会优先承载播放时按需检测出来的 season 级片头区间；`episodes[].intro_*` 字段已预留，但当前默认仍为空，方便后续再扩成单集覆盖。
- `episodes[].playback_progress` 会带上该集最近一次播放快照，前端可以据此显示集卡进度、已看完状态，以及“最近一集已播完则默认跳下一集”的续播入口。
- 可直接用于前端“可播放集高亮、缺失集置灰”的展示逻辑。
- 当前会把 TMDB 剧集大纲缓存到 PostgreSQL（`series_episode_outline_cache`），默认 TTL 为 24 小时，避免每次请求都访问 TMDB。
- 当缓存过期且 TMDB 临时不可用时，会回退到旧缓存并继续返回可用结果。

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
- 当前只支持对 `movie` 和 `series` 做人工匹配；`episode` 不支持单独匹配
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
    "provider_item_id": 1100988,
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
  "provider_item_id": 1100988
}
```

说明：
- 当前会把选中的 TMDB 条目 ID 持久化到 `media_items.metadata_provider_item_id`，并把 `metadata_status` 更新为 `matched`
- 后续该媒体条目的演员数据和剧集 outline 会优先按这个精确 TMDB ID 拉取，而不是再走模糊搜索
- 命中的远程图片会优先缓存到本地后写回；如果选中条目没有 `poster_path` / `backdrop_path`，对应字段会保持为空，不再沿用旧图或用其它层级图片兜底
- 如果当前条目是剧集，确认替换后会立即拉取该剧的远端季 / 集大纲，并把本地已存在季、已存在集的标题、简介、季海报和集封面写回数据库；远端季 / 集图会先缓存到本地再覆盖旧图，远端缺图时对应字段会清空
- 当前若所属媒体库正在扫描或正在删除，会返回 `409 Conflict`

返回：
- 成功时 `200 OK`
- 返回更新后的 `MediaItemResponse`

### `POST /api/media-items/{id}/refresh-metadata`

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
- 如果内置 TMDB token 可用，会继续按“本地优先，远程补空字段”的规则补齐缺失 metadata
- 刷新后会同步更新 `metadata_status`、`metadata_failure_reason` 和 `remote_media_type`
- 如果命中远程图片，仍会优先缓存到本地后再写回 `poster_path` / `backdrop_path`；远端缺失的图片字段会保持为空，不再由同条目的其它图片字段或其它层级图片补齐
- 如果该媒体条目之前已经通过 `POST /api/media-items/{id}/metadata-match` 绑定过精确 TMDB 条目，后续演员数据和剧集 outline 仍会沿用该绑定
- 如果源文件已经被重命名、移动或删除，当前会返回 `409 Conflict` 并要求你重扫库
- 当前若该媒体条目所属媒体库正在扫描或正在删除，会返回 `409 Conflict`
- 当前只支持单条媒体项刷新，不提供整库级 metadata refresh

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
- 当前会服务本地 sidecar 图片以及已缓存到本地的 TMDB 图片
- 如果极少数情况下缓存失败，详情接口里的 `poster_path` 仍可能是远程 TMDB 图片地址；这时前端应直接使用那个 URL，不需要再请求本接口
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
- 当前会服务本地 sidecar 图片以及已缓存到本地的 TMDB 图片
- 如果极少数情况下缓存失败，详情接口里的 `backdrop_path` 仍可能是远程 TMDB 图片地址；这时前端应直接使用那个 URL，不需要再请求本接口
- 如果该媒体条目没有背景图，返回 `404 Not Found`

### `GET /api/seasons/{id}/poster`

作用：
- 返回某一季的海报图片文件

路径参数：
- `id`：`season_id`

返回：
- 成功时返回 `200 OK`
- 响应体为图片内容，不是 JSON

说明：
- 当前会服务本地缓存图片或 sidecar 图片
- 如果 `poster_path` 是远程 URL，前端应直接使用 URL，不需要再请求本接口
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
- 当前会服务本地缓存图片或 sidecar 图片
- 如果 `backdrop_path` 是远程 URL，前端应直接使用 URL，不需要再请求本接口
- 如果该季没有背景图，返回 `404 Not Found`

## 6. 播放进度

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
- `media_file_id`：最近播放的文件 ID
- `position_seconds`：当前记录的播放秒数
- `duration_seconds`：记录的总时长
- `last_watched_at`：最近一次上报时间
- `is_finished`：是否标记为已看完

说明：
- `null` 是这个接口的正常语义，表示“当前用户还没有这条内容的播放记录”，不应当被当成异常
- 前端播放器当前会在播放中按 `5s` 心跳上报，并在暂停、播放结束、切源、切集、页面隐藏和离开页面时额外强制 flush 一次

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
- `playback_progress` 只保留“当前最新状态”，不承担完整历史时间线
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
      "original_title": null,
      "sort_title": null,
      "year": 1999,
      "overview": null,
      "poster_path": "/api/media-items/5/poster",
      "backdrop_path": "/api/media-items/5/backdrop",
      "created_at": "...",
      "updated_at": "..."
    },
    "playback_progress": {
      "id": 3,
      "media_item_id": 5,
      "media_file_id": 5,
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

## 7. 媒体流

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
- 当前只列出扫描时通过 `ffprobe` 发现的内嵌音轨
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
- 详情页当前会把 `/files`、`/audio-tracks`、`/subtitles` 三组数据组合成视频卡、音轨卡和字幕卡
- 音轨卡和字幕卡都通过卡头小下拉切换当前展示的轨道/字幕，不会把所有轨道一次性堆成很多张卡

说明：
- 服务端会把外挂字幕和内嵌字幕统一列在这里，前端播放器只需要渲染一份字幕菜单
- 外挂字幕当前支持：
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

说明：
- `srt` 会在服务端直接转换成 `WebVTT`
- `ass/ssa` 会借助 `ffmpeg` 转成 `WebVTT`
- 内嵌字幕会按流索引抽取后再转成 `WebVTT`
- 前端播放器切换字幕时，应只激活一条字幕轨道，避免外挂和内嵌字幕同时显示造成重影
- 如果单条字幕流转换或加载失败，客户端应提示该字幕不可用并继续播放主视频，而不是把整个播放器判成失败

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
- 带 `Range` 时为 `206 Partial Content`
- 响应体是文件流，不是 JSON

关键响应头：
- `Accept-Ranges: bytes`
- `Content-Type`
- `Content-Length`
- `Content-Range`（分段请求时）

说明：
- 当前应直接把这个 URL 给播放器使用
- 不建议前端先 `fetch` 完整文件再转 `blob`
- 当带上 `audio_track_id` 时，服务端会先验证这条音轨确实属于当前媒体文件，再按 `ffmpeg -c copy` 生成缓存变体；这里是 remux，不是转码
- 当前 remux 变体仍然只服务于源码直放，不会提供多码率或自适应码流

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
- `200 OK` 或 `206 Partial Content`
- 没有响应体

说明：
- 前端通常不需要手动调用
- 浏览器播放器可能会自己使用
- 如果请求的是某条音轨变体，服务端会先确保对应缓存变体已经准备好

## 8. ID 关系说明

当前前端最容易混淆的是这三个 ID：

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
