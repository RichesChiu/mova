# Mova API 一览

这份文档整理当前 `mova-server` 已实现的全部 HTTP 接口，重点说明每个接口的用途、关键入参和返回语义。

## 通用说明

- Base URL：默认 `http://127.0.0.1:36080`
- 响应格式：
  - 普通业务接口默认返回 JSON，并统一包裹成 `code / message / data`
  - 媒体流和图片资源接口返回文件流，不返回 JSON
- 鉴权：
  - `GET /api/health`、`GET /api/auth/bootstrap-status`、`POST /api/auth/bootstrap-admin`、`POST /api/auth/login`、`POST /api/auth/token-login` 可匿名访问
  - 其他接口都要求登录态
  - Web 端继续使用 session cookie
  - 原生客户端可使用 `Authorization: Bearer <token>`，token 通过 `POST /api/auth/token-login` 获取
  - 管理类接口（用户管理、建库、删库、触发扫描、服务器根目录等）要求 `admin`
  - `GET /api/events` 返回 `text/event-stream`，不使用统一 JSON envelope
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

- 文档中后续字段示例多数只展示 `data` 内部结构，真实响应会额外包一层统一 envelope。

- 常见状态码：
  - `200 OK`：请求成功
  - `201 Created`：创建成功
  - `202 Accepted`：异步任务已创建并开始后台执行
  - `400 Bad Request`：请求参数或业务校验不通过
  - `401 Unauthorized`：未登录或 session 已失效
  - `403 Forbidden`：已登录但没有权限访问
  - `409 Conflict`：资源当前状态不允许执行该操作
  - `404 Not Found`：资源不存在
  - `416 Range Not Satisfiable`：媒体流的 `Range` 请求越界
  - `500 Internal Server Error`：服务内部错误
- TMDB provider 现在从运行时环境变量 `MOVA_TMDB_ACCESS_TOKEN` 读取；但每个媒体库仍可单独配置 `metadata_language`，决定扫描与元数据补全时使用 `zh-CN` 或 `en-US`。
- 如果额外配置了可选的 `MOVA_OMDB_API_KEY`，服务端会在已拿到 `imdb_id` 的前提下补齐 `imdb_rating`；不配置时该字段保持为空，不影响扫描、入库和播放。
- 本地海报和背景图这类图片资源，对外返回的 URL 现在会带版本参数（例如 `/api/media-items/42/poster?v=1704164645`），浏览器可以长期缓存；当媒体元数据更新时，版本参数会变化，前端会自动拿到新图。

## 接口总览

| Method | Path | 作用 |
| --- | --- | --- |
| `GET` | `/api/health` | 健康检查 |
| `GET` | `/api/auth/bootstrap-status` | 查询是否需要初始化首个管理员 |
| `POST` | `/api/auth/bootstrap-admin` | 初始化首个管理员并登录 |
| `POST` | `/api/auth/login` | 登录 |
| `POST` | `/api/auth/token-login` | 为原生客户端创建 Bearer token |
| `POST` | `/api/auth/logout` | 登出 |
| `GET` | `/api/auth/me` | 查询当前用户 |
| `PATCH` | `/api/auth/me` | 更新当前用户昵称 |
| `GET` | `/api/events` | 订阅服务端实时事件流（SSE） |
| `PUT` | `/api/auth/password` | 当前用户修改自己的密码 |
| `GET` | `/api/users` | 查询用户列表（管理员） |
| `POST` | `/api/users` | 创建用户（管理员） |
| `PATCH` | `/api/users/{id}` | 更新用户基础信息（管理员） |
| `DELETE` | `/api/users/{id}` | 删除用户（管理员） |
| `PUT` | `/api/users/{id}/password` | 管理员重置指定用户密码 |
| `PUT` | `/api/users/{id}/library-access` | 更新普通用户的媒体库访问范围（管理员） |
| `GET` | `/api/libraries` | 查询媒体库列表 |
| `POST` | `/api/libraries` | 创建媒体库 |
| `GET` | `/api/libraries/{id}` | 查询单个媒体库详情 |
| `PATCH` | `/api/libraries/{id}` | 更新媒体库基础配置 |
| `DELETE` | `/api/libraries/{id}` | 删除媒体库 |
| `GET` | `/api/libraries/{id}/media-items` | 查询媒体库下的媒体条目列表 |
| `GET` | `/api/libraries/{id}/scan-jobs` | 查询媒体库扫描历史 |
| `GET` | `/api/libraries/{id}/scan-jobs/{scan_job_id}` | 查询单个扫描任务状态 |
| `POST` | `/api/libraries/{id}/scan` | 触发异步扫描 |
| `GET` | `/api/media-items/{id}` | 查询单个媒体条目详情 |
| `GET` | `/api/media-items/{id}/cast` | 查询单个媒体条目的演员列表 |
| `GET` | `/api/media-items/{id}/playback-header` | 查询播放器页头部信息 |
| `GET` | `/api/media-items/{id}/files` | 查询媒体条目关联文件列表 |
| `GET` | `/api/media-items/{id}/seasons` | 查询某个剧集条目的季列表 |
| `GET` | `/api/media-items/{id}/episode-outline` | 查询剧集全集大纲并标记本地可用集 |
| `GET` | `/api/media-items/{id}/metadata-search` | 手动搜索单条媒体的候选元数据（管理员） |
| `POST` | `/api/media-items/{id}/metadata-match` | 选择候选结果并替换当前媒体元数据（管理员） |
| `POST` | `/api/media-items/{id}/refresh-metadata` | 手动重拉单个媒体条目元数据 |
| `GET` | `/api/media-items/{id}/poster` | 读取媒体条目海报图 |
| `GET` | `/api/media-items/{id}/backdrop` | 读取媒体条目背景图 |
| `GET` | `/api/seasons/{id}/episodes` | 查询某一季下的集列表 |
| `GET` | `/api/seasons/{id}/poster` | 读取某一季海报图 |
| `GET` | `/api/seasons/{id}/backdrop` | 读取某一季背景图 |
| `GET` | `/api/media-items/{id}/playback-progress` | 查询单条内容的最近播放进度 |
| `PUT` | `/api/media-items/{id}/playback-progress` | 写入或更新播放进度 |
| `GET` | `/api/playback-progress/continue-watching` | 查询继续观看列表 |
| `GET` | `/api/watch-history` | 查询当前用户自己的观看历史 |
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
- 当前用户名精确匹配
- 密码最少 8 位
- 成功后会写入 session cookie

### `POST /api/auth/token-login`

作用：
- 使用用户名和密码登录，并直接返回给原生客户端可复用的 Bearer token

请求体：

```json
{
  "username": "admin",
  "password": "admin123456"
}
```

返回：

```json
{
  "token": "native-client-session-token",
  "token_type": "Bearer",
  "expires_at": "2026-05-20T08:15:30Z",
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
- 当前会直接复用服务端现有 session 机制，只是把 token 直接返回给客户端，而不是写 cookie
- 后续请求把 `Authorization: Bearer <token>` 带到受保护接口即可
- token 过期、用户被禁用或被删除后，这个 token 会失效
- Web 端不需要使用这个接口，仍然继续调用 `POST /api/auth/login`

### `POST /api/auth/logout`

作用：
- 删除当前登录态对应的服务端会话记录；如果当前是 cookie 登录，还会顺带清理 session cookie

返回：
- `200 OK`

说明：
- 支持 cookie 和 Bearer token 两种登录态
- 如果同时带了 cookie 和 `Authorization`，服务端会优先使用 Bearer token

### `GET /api/auth/me`

作用：
- 查询当前登录用户

返回：
- `200 OK`
- 返回字段包括 `id`、`username`、`nickname`、`role`、`is_primary_admin`、`is_enabled`、`library_ids`
- 支持 cookie 和 Bearer token 两种登录态
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

### `GET /api/events`

作用：
- 订阅服务端实时事件流，用于把扫描任务状态变化、媒体库更新和元数据变更主动推送给在线客户端

说明：
- 需要登录态
- 支持 cookie 和 Bearer token 两种登录态
- 返回类型为 `text/event-stream`
- 事件本身更适合作为“资源已变化”的通知；客户端收到后，应按事件类型重新拉对应 HTTP 接口，而不是把 SSE 负载直接当成最终页面数据
- 当前服务端只推送连接建立之后的新事件；客户端断线后应自动重连，并在重连成功后主动补一轮关键查询，避免断开期间漏掉一次刷新
- 当前已实现事件：
  - `scan.job.updated`
  - `scan.job.finished`
  - `scan.item.updated`
  - `library.updated`
  - `library.deleted`
  - `media_item.metadata.updated`
- `scan.job.updated` / `scan.job.finished` 里的 `scan_job` 目前会额外带一个可选 `phase` 字段，当前会使用：
  - `discovering`：正在发现文件
  - `enriching`：正在补全元数据和图片
  - `syncing`：正在写入媒体库
  - `finished`：任务已结束
- `scan.item.updated` 用于扫描中的条目级提示；现在从“刚发现新的电影文件或剧集目录组”开始就会推送，同一个 `item_key` 会在后续元数据、图片和写库前阶段持续更新，当前结构如下：

```json
{
  "type": "scan.item.updated",
  "item": {
    "scan_job_id": 41,
    "library_id": 7,
    "item_key": "/media/series/Arcane",
    "media_type": "series",
    "title": "Arcane",
    "season_number": null,
    "episode_number": null,
    "item_index": 12,
    "total_items": 240,
    "stage": "artwork",
    "progress_percent": 72
  }
}
```

字段说明：
- `item_key`：当前扫描条目的稳定键；电影当前直接使用文件路径，剧集会优先使用系列目录路径，避免前端在扫描中先看到一集一集被打散
- `item_index` / `total_items`：当前条目在整批扫描里的位置，用于前端估算总进度
- `stage`：当前条目处理阶段，当前会使用 `discovered` / `metadata` / `artwork` / `completed`
- `progress_percent`：当前条目自身的粗粒度进度百分比，便于前端直接驱动占位卡进度条
- 前端可以把同一个 `item_key` 当成一张临时扫描卡：发现文件或目录组时先渲染出来，后续收到新事件后只更新这张卡，而不是整块列表突然出现或突然消失
- 当某个远端元数据步骤失败但扫描继续时，当前仍以服务端日志为主；日志会明确标出是 metadata enrichment 阶段失败，并说明会回退到本地数据

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
- 如果当前是 Bearer token 客户端，修改密码成功后应使用新密码重新调用 `POST /api/auth/token-login` 获取新 token

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
- `library_ids` 只对 `viewer` 生效；更新为 `admin` 时会自动清空库授权

关键约束：
- 当前用户不能通过该接口禁用自己
- 当前用户不能通过该接口修改自己的角色
- 不能降级、禁用最后一个启用中的管理员
- 禁用用户后，服务端会清理该用户现有 session
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
- `204 No Content`

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
- 重置成功后，该用户现有 session 会全部失效
- 只有主管理员可以重置普通管理员密码

### `PUT /api/users/{id}/library-access`

作用：
- 更新某个普通用户的媒体库授权范围

请求体：

```json
{
  "library_ids": [1, 2]
}
```

说明：
- 对 `admin` 调用时会被忽略，管理员仍然默认拥有全部媒体库权限
- 普通管理员仍然可以管理 `viewer` 的媒体库授权
- 普通管理员不能通过该接口修改任何管理员的库授权

## 3. 媒体库

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
- `library_type`：媒体库类型，当前支持 `mixed` / `movie` / `series`
- `metadata_language`：该媒体库扫描和 TMDB 补全时使用的语言，当前支持 `zh-CN` / `en-US`
- `root_path`：扫描根目录
- `is_enabled`：是否启用

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
  "library_type": "mixed",
  "metadata_language": "zh-CN",
  "root_path": "/data/media",
  "is_enabled": true
}
```

字段说明：
- `name`：媒体库名称
- `description`：可选，媒体库描述
- `library_type`：媒体库类型，支持 `mixed` / `movie` / `series`
- `metadata_language`：TMDB 元数据语言，当前支持 `zh-CN` / `en-US`，不传时默认 `zh-CN`
- `root_path`：要扫描的本地目录
- `is_enabled`：可选，不传时默认为 `true`

关键校验：
- 名称不能为空
- 类型不能为空
- 路径不能为空
- 路径必须存在且必须是目录

返回：
- 成功时 `201 Created`
- 返回创建后的 `LibraryResponse`

说明：
- 创建媒体库不会自动开始扫描；后续需要显式调用 `POST /api/libraries/{id}/scan`
- `is_enabled = false` 只表示这个媒体库当前处于禁用状态，不再承担“自动监听/自动同步”的语义
- 当前允许重叠或完全相同的 `root_path`。同一个物理文件如果被多个库路径覆盖，会在各自库里独立建模和展示。
- `mixed` 是推荐默认值。它允许一个目录里同时放电影和剧集，扫描时会按单个文件自动判断：
  - 命中 `S01E02`、`1x02`、`Season 01/` 这类信号时按剧集处理
  - 其他文件默认按电影处理
- `movie` 会把该库所有视频文件都按电影处理
- `series` 会把该库所有视频文件都按剧集处理

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
- `last_scan.phase`：当前 HTTP 查询里通常为 `null`；实时扫描阶段会通过 `GET /api/events` 的 `scan.job.*` 事件补齐

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
- 删除成功时返回 `204 No Content`

说明：
- 删除前服务会先把该库标记为“正在删除”，阻止新的扫描请求进入
- 如果当前库有正在执行的扫描任务，服务会先请求取消并等待它退出，再真正删除库
- 删除 `libraries` 记录后，关联的 `scan_jobs`、`media_items`、`media_files` 等数据会通过数据库外键级联删除
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
  "metadata_language": "en-US",
  "is_enabled": true
}
```

字段说明：
- `name`：可选，更新媒体库名称
- `description`：可选，更新媒体库描述；传 `null` 可清空现有描述
- `metadata_language`：可选，更新 TMDB 元数据语言，当前支持 `zh-CN` / `en-US`
- `is_enabled`：可选，控制该媒体库是否启用

返回：
- 成功时 `200 OK`
- 返回更新后的 `LibraryResponse`

说明：
- 至少要传一个字段，否则返回 `400 Bad Request`
- 更新名称、描述或元数据语言不会直接重扫，但后续手动扫描会按新配置继续执行
- 当 `is_enabled` 从 `true` 改成 `false` 时，如果该库此时有扫描正在执行，会先请求取消当前扫描

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
- `phase`：当前 HTTP 查询里通常为 `null`；实时扫描阶段会通过 `scan.job.updated` / `scan.job.finished` 填充
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
- 扫描在后台执行，前端应拿返回的 `scan_job.id` 去轮询 `/api/libraries/{id}/scan-jobs/{scan_job_id}`
- 当前扫描会按 `(library_id, file_path)` 做增量同步：同路径文件原地更新，缺失路径删除，改名或移动会表现成旧路径删除加新路径新增
- 现在只有手动扫描会驱动这套库存对齐与元数据补全链路；新增、删除、改名和移动都会在手动扫描时收敛出来

## 4. 媒体条目

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
- `imdb_rating`：可选的 IMDb 评分字符串；只有在配置了 `MOVA_OMDB_API_KEY` 且当前条目能解析到 `imdb_id` 时才会有值
- `country`：可选的国家/地区信息；电影会优先使用 TMDB 的 production countries，剧集会优先使用 TMDB 的 origin country
- `genres`：可选的题材类型字符串；来自 TMDB genres，会按展示顺序拼接
- `studio`：可选的制作公司字符串；来自 TMDB production companies，会按展示顺序拼接
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

说明：
- 当前前端播放时应先从这个接口拿到 `media_file_id`
- 如果服务运行环境里安装了 `ffprobe`，扫描时会尽量填充时长、编码、分辨率和码率
- 如果没有安装 `ffprobe`，或者文件探测失败，这些字段会保持为空，但不会阻断扫描
- 如果这个条目是 `series`，这里通常返回空列表；请改用 `/api/media-items/{id}/seasons`

### `GET /api/media-items/{id}/seasons`

作用：
- 查询某个剧集条目下的季列表

路径参数：
- `id`：`series media_item_id`

返回：
- `200 OK`
- 返回 `SeasonResponse[]`

说明：
- 只有 `media_type = series` 的条目适用
- 每条季记录会带 `episode_count`
- `overview` 为该季简介（若已从 sidecar/TMDB 补齐）
- 现在会返回 `intro_start_seconds` / `intro_end_seconds`，可作为该季默认片头区间
- 现在会返回 `poster_path` / `backdrop_path`：
  - 本地文件会映射成 `/api/seasons/{id}/poster` 或 `/api/seasons/{id}/backdrop`
  - 远程图片（例如 TMDB）保持原始 URL

### `GET /api/seasons/{id}/episodes`

作用：
- 查询某一季下的集列表

路径参数：
- `id`：`season_id`

返回：
- `200 OK`
- 返回 `EpisodeResponse[]`

说明：
- 返回的是该季下的聚合集列表
- `media_item_id` 可继续用于详情、文件列表、播放进度和播放接口
- `overview` 为该集简介（来自本地 sidecar 或 TMDB）
- 现在会返回 `intro_start_seconds` / `intro_end_seconds`，可用于覆盖季级默认片头区间
- 每集记录会返回 `poster_path` / `backdrop_path`（优先用集级图，缺失时可能为空）

### `GET /api/media-items/{id}/episode-outline`

作用：
- 查询剧集“全集大纲 + 本地可用性”

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
  - `seasons[].backdrop_path`
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
- TMDB 侧目前可直接提供季海报（`season poster`）和集剧照（`episode still`）；集背景图与封面会复用剧照。
- 若集级图片缺失，后端会尝试从本地视频抽取第一帧作为回退（需运行环境可用 `ffmpeg`），并避免把通用目录海报（如 `poster.jpg` / `folder.jpg`）误当成单集封面。
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
- 当前会把选中的 TMDB 条目 ID 持久化到 `media_items.metadata_provider_item_id`
- 后续该媒体条目的演员数据和剧集 outline 会优先按这个精确 TMDB ID 拉取，而不是再走模糊搜索
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
- 如果命中远程图片，仍会优先缓存到本地后再写回 `poster_path` / `backdrop_path`
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

## 5. 播放进度

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
- 每次成功写入都会同步刷新该用户的 `watch_history`
- 当 `is_finished = false` 时，该条内容会继续出现在 `continue-watching`
- 当 `is_finished = true` 时，该条内容会从 `continue-watching` 移除，但对应观看会话仍会保留在 `watch-history`

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
- 按最近观看时间倒序返回
- 电影按 `media_item` 聚合；剧集会按 `series` 聚合
- 同一部剧无论看了哪一季哪一集，都只保留最近观看的那一集
- 如果条目来自剧集，`season_number` / `episode_number` / `episode_title` 会标识最近观看的具体集数
- 如果条目来自剧集，`episode_overview` / `episode_poster_path` / `episode_backdrop_path` 会返回最近观看那一集的描述和封面
- 默认返回 `20` 条，最大 `100` 条

### `GET /api/watch-history`

作用：
- 查询当前登录用户自己的观看历史会话列表

查询参数：
- `limit`：可选，返回条目数量上限

示例：
- `/api/watch-history`
- `/api/watch-history?limit=50`

返回：
- `200 OK`
- 返回 `WatchHistoryItemResponse[]`

关键字段：
- `watch_history.started_at`：本次观看会话开始时间
- `watch_history.last_watched_at`：最后一次上报时间
- `watch_history.ended_at`：会话结束时间；未结束时为空
- `watch_history.completed_at`：本次会话明确看完时的时间；未看完时为空
- `watch_history.is_finished`：是否在该次观看会话内看完

说明：
- 这是独立于 `playback_progress` 的历史表，一条记录代表一次观看会话
- 同一用户、同一文件在短时间内连续上报会复用同一条历史，避免被播放心跳刷出大量碎片记录
- 如果间隔较久后重新开始观看，同一文件会新开一条历史记录
- 历史记录同样按当前登录用户隔离，不会和其他用户共享

## 6. 媒体流

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

## 7. ID 关系说明

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
5. 播放时：
   - 默认音轨：`<video src="/api/media-files/{media_file_id}/stream" />`
   - 切换音轨后：`<video src="/api/media-files/{media_file_id}/stream?audio_track_id={audio_track_id}" />`
   - `PUT /api/media-items/{media_item_id}/playback-progress`
