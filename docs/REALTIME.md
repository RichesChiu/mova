# Mova Realtime / SSE 契约

本文档是 Mova Web、macOS、iOS 和 iPadOS 客户端共同遵循的实时同步契约，描述首个正式开发契约 `protocol_version = 1` 的实际实现、事件触发条件、数据使用方式、断线恢复流程和已知架构边界。开发过程中未发布的旧格式不保留兼容路径，客户端以服务端当前契约为唯一事实源。

接口字段和业务 API 总览仍以 [`API.md`](API.md) 为准。本文档专门解释 realtime state 与 SSE，不把某个客户端的 React Query、SwiftUI 或页面实现当成协议的一部分。

## 1. 设计目标

Mova 的实时同步分成两条职责不同的链路：

1. **持久化资源版本**：负责最终业务数据的一致性。
2. **临时扫描事件**：负责展示扫描过程，允许合并和丢失。

```text
业务事务
  -> 同事务增加 resource revision
  -> PostgreSQL NOTIFY
  -> RealtimeDispatcher 合并
  -> resources.changed
  -> 客户端按 resource 调用业务 API

扫描运行时
  -> ScanJobEvent
  -> RealtimeDispatcher latest-wins 合并
  -> scan.progress / scan.finished
  -> 客户端更新临时扫描 UI
```

核心约束：

- SSE 不是最终业务数据源。
- 电影、剧集、媒体库、用户资料和继续观看的最终状态必须通过普通业务 API 获取。
- 客户端不需要收到每一条 SSE 事件，也不依赖事件顺序保证最终正确。
- 资源 revision 是可靠状态；PostgreSQL `NOTIFY` 和 SSE 都只是低延迟唤醒信号。
- `scan.progress` 是临时 UI 数据，不做历史回放。
- 同一套协议面向 Web 与原生客户端，不约定具体缓存框架。

## 2. 接口

### 2.1 `GET /api/realtime/events`

建立 SSE 长连接，响应类型为 `text/event-stream`，不使用普通 API 的 `code / message / data` JSON envelope。

鉴权方式：

- Web：session cookie。
- macOS / iOS / iPadOS：`Authorization: Bearer <access_token>`。
- refresh token 不能用于建立 SSE 连接。

服务端每 15 秒发送一次 keep-alive。keep-alive 不是业务事件，客户端不需要更新任何数据。

当前协议不提供 SSE `id`，也不支持 `Last-Event-ID` 历史回放。重连恢复统一使用 `/api/realtime/state`。

### 2.2 `GET /api/realtime/state`

返回当前用户有权看到的可靠 realtime 状态，使用普通 JSON envelope。`data` 结构如下：

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

字段语义：

- `protocol_version`：当前为 `1`。客户端遇到不支持的版本时不得猜测事件含义，应停止消费并提示升级。
- `server_epoch`：当前数据库生命周期标识。服务重启不会改变；数据库重建会改变。
- `resources`：当前用户可见资源的最新 revision。尚未变化过的资源返回 `0`。
- `active_scans`：当前仍为 `pending` 或 `running` 的扫描任务，只恢复任务级状态，不回放历史条目进度。

### 2.3 `GET /api/home` 中的 realtime 基线

`GET /api/home` 会同时返回：

```json
{
  "realtime": {
    "protocol_version": 1,
    "server_epoch": "019f...",
    "resources": {
      "admin:libraries": 14,
      "user:12:continue-watching": 39
    }
  }
}
```

客户端可以把它作为**首页读模型**对应的已应用 revision 基线，避免首页刚加载完又因为相同 revision 重复刷新。它不能证明媒体详情、用户管理或其他独立读模型已经应用相同 revision；这些页面首次加载时仍应读取自己的业务 API，已存在的活跃缓存则应在首次 state 对账时标记失效。

## 3. 资源键与触发条件

资源键表示需要重新读取的业务聚合，不表示单个数据库行。

| 资源键 | 当前触发条件 | 可见范围 | 客户端应重新读取 |
| --- | --- | --- | --- |
| `admin:libraries` | 媒体库插入或删除 | 管理员 | 管理员可见的媒体库集合；需要时同步首页 |
| `library:{id}:settings` | 指定媒体库更新或删除 | 有权访问该库的用户 | 媒体库详情、列表中的该库摘要、首页该库摘要 |
| `library:{id}:catalog` | `media_items`、`media_files`、`seasons` 或 `episodes` 插入、更新、删除 | 有权访问该库的用户 | 库目录、最近添加、首页预览；当前打开媒体的详情、演员、资源版本、播放头部或剧集大纲 |
| `library:{id}:scan` | 扫描任务入队，或任务状态在 `pending` / `running` / `success` / `failed` 之间发生变化 | 有权访问该库的用户 | 库详情、扫描状态，并通过 realtime state 增加或清理 active scan |
| `user:{id}:libraries` | 用户媒体库授权插入或删除 | 指定用户 | 当前用户可见媒体库列表和首页 |
| `user:{id}:profile` | 指定用户插入、更新或删除 | 指定用户 | 当前用户资料和包含用户摘要的页面 |
| `user:{id}:continue-watching` | 继续观看队列插入、更新或删除 | 指定用户 | 继续观看列表和首页继续观看区域 |
| `admin:users` | 用户插入、更新或删除 | 管理员 | 用户管理列表 |

内部还存在 `session:user:{id}` 唤醒键。它不作为 `resources.changed` 暴露，也不出现在 realtime state 中；服务端会把它转换为 `session.invalidated`。

当前触发 `session:user:{id}` 的操作包括：

- 用户被删除。
- 用户角色改变。
- 用户启用状态改变。
- 密码哈希改变。
- 用户媒体库访问权限改变。

## 4. SSE 事件总览

| 事件 | 是否携带最终业务数据 | 是否允许丢失 | 典型用途 |
| --- | --- | --- | --- |
| `resources.changed` | 否，只携带资源键和 revision | 通知可丢，revision 不丢 | 定向调用业务 API 刷新数据 |
| `scan.progress` | 否，携带临时扫描展示数据 | 是 | 实时展示扫描任务和扫描卡片 |
| `scan.finished` | 否，携带扫描终态和最后一批临时条目 | Dispatcher 输入端不因饱和丢弃；仍需 revision 恢复兜底 | 立即显示终态，刷新正式目录后移除临时卡片 |
| `resync.required` | 否 | 连接随后关闭 | 要求客户端重新读取 realtime state |
| `session.invalidated` | 否 | 连接随后关闭 | 停止使用旧权限并重新建立登录态 |

## 5. `resources.changed`

### 5.1 触发流程

1. 业务表 mutation 触发数据库 trigger。
2. trigger 在同一事务中增加 `realtime_revisions.revision`。
3. trigger 调用 `pg_notify('mova_realtime', resource_key)`。
4. 事务提交后，当前服务实例的 PostgreSQL Listener 收到通知。
5. `RealtimeDispatcher` 按资源键去重并读取数据库中的最新 revision。
6. Dispatcher 按权限范围组成批次并发送 `resources.changed`。

如果业务事务回滚，revision 和通知都会一起回滚。

### 5.2 合并频率

- 普通资源最多每 500ms 发送一批。
- 继续观看默认最多每 1 秒发送一批。
- 标记已看完后，服务端会额外要求立即发送继续观看 revision。
- 同一窗口内同一资源发生多次变化，只发送数据库中的最新 revision。

### 5.3 Payload

```text
event: resources.changed
data: {
  "protocol_version": 1,
  "changes": [
    {
      "resource": "library:7:catalog",
      "revision": 128
    },
    {
      "resource": "user:12:continue-watching",
      "revision": 39
    }
  ]
}
```

不同权限范围可能拆成多个 SSE 批次，客户端不能假设一次事件包含同一时刻的所有资源。

### 5.4 客户端使用规则

客户端为每个资源至少维护：

- `applied_revision`：本地业务数据已经成功同步到的 revision。
- `requested_revision`：已经收到但尚未完成同步的最高 revision。
- `refresh_in_flight`：该资源是否正在同步。

处理规则：

1. `revision <= applied_revision`：忽略重复或乱序旧事件。
2. `revision > requested_revision`：保存更高 revision。
3. 同一资源只允许一个刷新任务运行。
4. API 刷新成功后才能推进 `applied_revision`。
5. 刷新期间收到更高 revision，当前任务完成后继续同步到最高 revision。
6. 刷新失败时保留 dirty 状态，执行退避重试或重新调用 realtime state，不得把失败 revision 标记为已应用。

这里的“已应用”是指该客户端当前已加载的相关读模型已经刷新或被可靠地标记为 stale，不表示客户端必须预先下载一个媒体库的全部分页数据。

对于当前没有加载到内存的页面，客户端可以只把资源标记为 dirty，在用户进入页面时读取；但不能把旧缓存标记为已经同步到新 revision。

## 6. `scan.progress`

### 6.1 触发条件

扫描运行时发生以下变化时会产生临时事件：

- 扫描任务启动或 phase 改变。
- 文件发现数量达到服务端节流阈值。
- 一个扫描组完成本地分析并以 `metadata_status = pending` 写入数据库。
- 一个扫描组进入 metadata 请求阶段。
- 一个扫描组进入 artwork 请求阶段。
- 一个扫描组完成最终入库。

Dispatcher 按扫描任务合并，最多每 200ms 发送一批：

- `scan_job` 只保留最新任务状态。
- 相同 `(scan_job_id, item_key)` 只保留最新条目状态。
- Dispatcher 输入队列饱和时，普通临时进度允许丢弃。

### 6.2 Payload

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
    "scanned_files": 52,
    "created_at": "2026-07-14T00:00:00Z",
    "started_at": "2026-07-14T00:00:01Z",
    "finished_at": null,
    "error_message": null
  },
  "items": [
    {
      "scan_job_id": 41,
      "library_id": 7,
      "item_key": "series-title:arcane",
      "media_type": "series",
      "title": "Arcane",
      "year": 2024,
      "overview": null,
      "poster_path": "/api/media-items/1860/poster?v=1",
      "backdrop_path": null,
      "metadata_status": "pending",
      "remote_media_type": "series",
      "season_number": null,
      "episode_number": null,
      "item_index": 52,
      "total_items": 240,
      "stage": "artwork",
      "progress_percent": 76
    }
  ]
}
```

扫描任务 phase：

- `initializing`
- `discovering`
- `analyzing`
- `enriching`
- `syncing`
- `finished`

扫描条目 stage 与当前展示百分比：

| stage | 当前百分比 | 含义 |
| --- | ---: | --- |
| `discovered` | 6 | 已识别本地结构并写入初步数据 |
| `metadata` | 36 | 正在获取或判断远端元数据 |
| `artwork` | 76 | 正在获取海报、背景图等资源 |
| `completed` | 100 | 该组最终数据已经写入数据库 |

百分比是 UI 展示提示，不是可以恢复的任务进度账本。客户端不得根据缺失的中间百分比判断扫描失败。

### 6.3 客户端使用规则

- 按 `library_id` 保存当前扫描运行时。
- 按 `item_key` latest-wins 合并条目。
- 新的 `scan_job.id` 出现时，丢弃该库旧任务的临时条目。
- `poster_path` 和 `backdrop_path` 只用于临时展示，正式卡片仍以业务 API 返回值为准。
- 客户端应限制内存中的临时条目数量；当前 Web 每个媒体库最多保留 40 个。
- 收不到某个条目阶段是正常情况，不要轮询补齐历史 stage。

## 7. `scan.finished`

### 7.1 触发条件

扫描任务进入终态后立即触发，包括成功、失败和取消后写入的终态。

服务端会：

1. 从 Dispatcher 中取出该任务尚未发送的最新进度。
2. 合入最终 `scan_job`。
3. 读取本次终态对应的 `library:{id}:catalog` 与 `library:{id}:scan` revisions。
4. 使用和 `scan.progress` 相同的任务、条目字段，并增加 `changes` 后发送 `scan.finished`。
5. 如果 revision 查询暂时失败，仍发送终态事件，但 `changes` 为空，客户端转入 state reconcile。
6. 不等待 200ms 扫描进度窗口。

Dispatcher 输入队列已满时，终态事件会等待队列容量，不按普通进度丢弃。

### 7.2 客户端使用规则

典型 payload：

```text
event: scan.finished
data: {
  "protocol_version": 1,
  "scan_job": {
    "id": 41,
    "library_id": 7,
    "status": "success",
    "phase": "finished",
    "total_files": 240,
    "scanned_files": 240
  },
  "items": [],
  "changes": [
    {"resource": "library:7:catalog", "revision": 128},
    {"resource": "library:7:scan", "revision": 9}
  ]
}
```

当前 v1 顺序：

1. 先显示最终扫描状态。
2. 读取 payload 的 `changes`；其中包含服务端当前可读取到的 `library:{id}:catalog` 与 `library:{id}:scan` revision。
3. 把 change 交给处理 `resources.changed` 的同一个 Revision Coordinator。
4. 所有 change 对应的正式 API 刷新成功后，再移除尚未被正式媒体卡片替代的临时条目。
5. `changes` 为空或刷新失败时保留临时状态，并调用 realtime state 对账。

即使完全没有收到 `scan.finished`，客户端仍可通过以下可靠状态恢复：

- `library:{id}:scan` revision 已变化。
- `/api/realtime/state.active_scans` 中不再存在该任务。
- `library:{id}:catalog` revision 表示最终媒体数据变化。

## 8. `resync.required`

```text
event: resync.required
data: {"protocol_version":1,"reason":"client_lagged"}
```

当前服务端 SSE 最后一跳按 public、admin、library、user scope 分成容量为 32 个批次的有界广播队列。客户端只订阅与自身有关的 scope；消费相关队列明显落后，或 PostgreSQL Listener 订阅/重订阅成功需要关闭潜在通知空档时，服务端：

1. 发送一次 `resync.required`。
2. 关闭当前 SSE 流。

客户端收到后应：

1. 停止处理当前连接。
2. 重新建立 SSE。
3. 读取 realtime state。
4. 对比 revision，只同步不一致的资源。

不要尝试根据丢失事件数量逐条补事件。

## 9. `session.invalidated`

```text
event: session.invalidated
data: {"protocol_version":1,"reason":"authorization_changed"}
```

服务端发送后会关闭 SSE 流。客户端必须：

1. 停止使用连接建立时的旧权限快照。
2. 清理受权限影响的内存缓存。
3. Web 重新验证 session；原生客户端先使用当前 access token 重新获取用户资料，只有收到 `401` 时才使用 refresh token。
4. 认证失败时回到登录页。
5. 认证成功后重新读取用户资料、媒体库权限和 realtime state，再建立新连接。

## 10. 跨平台客户端状态机

### 10.1 冷启动

推荐顺序：

1. 完成认证。
2. 获取轻量首页或必要的首屏业务数据，并保存响应中的 realtime 基线。
3. 建立 SSE 连接。
4. SSE 打开后立即获取 realtime state。
5. state 请求期间，把收到的 `resources.changed` 按资源保存最高 revision。
6. 首次 state 对账时，刷新当前已加载的相关读模型，并把尚未激活的缓存标记为 stale；成功后才能接受 state revision 为本地基线。
7. 合并 state 和已缓冲事件，再执行差异同步。

不要先获取 state、随后才建立 SSE；两者之间发生的变更可能没有任何连接接收。

### 10.2 重连

- 使用指数退避并加入少量随机抖动，避免大量客户端同时重连。
- 每次连接重新打开后都读取 realtime state。
- 不依赖 `Last-Event-ID`。
- 同一登录用户、同一客户端进程通常只保留一条 SSE 连接，由共享 Sync Coordinator 分发给页面或 ViewModel。

### 10.3 iOS / iPadOS 后台与前台

- 进入后台时主动关闭 SSE，不要求后台持续保活。
- 回到前台后先建立 SSE，再获取 realtime state 并做差异同步。
- access token 过期时先刷新 token，再重新建立 SSE。
- 不要因为后台期间缺少扫描进度而全量下载所有媒体库目录。

### 10.4 Web

- 浏览器 `EventSource` 使用同源 cookie。
- 把资源键映射到缓存或 query key，但映射必须覆盖该资源影响的全部读模型。
- 一个批次包含多个资源时，应先合并需要失效的 query key，再执行刷新，避免多次刷新同一个首页查询。

### 10.5 macOS

- 与 iOS 共用认证、revision store、重连和资源同步逻辑。
- ViewModel 不应分别建立 SSE，也不应在任意事件后执行完整 `loadHomeData()`。
- 进入具体媒体库时才分页加载完整目录。

## 11. 权限与信息隔离

SSE 消息按以下范围过滤：

- server/public
- admin
- library
- user

服务端在建立连接时读取用户权限快照，并只把连接挂到相关 scope 的有界频道：

- `library:{id}:*` 只发送给有权访问该媒体库的用户。
- `user:{id}:*` 只发送给本人。
- `admin:*` 只发送给管理员。
- 管理员频道同时接收所有 library scope 事件，不需要为每个库建立独立订阅。
- 某个用户或媒体库的高频事件不会唤醒无关用户连接，也不会占用无关连接的背压缓冲。

权限改变时必须通过 `session.invalidated` 关闭旧连接，避免旧连接持续使用过期权限快照。

客户端仍然必须依赖普通业务 API 的鉴权，不能把“收到过某个 SSE 事件”当作访问资源的授权凭证。

## 12. 服务端当前限流与背压

当前实现参数：

| 项目 | 当前值 |
| --- | ---: |
| Dispatcher command queue | 2048 |
| 每个 SSE scope broadcast queue | 32 个批次 |
| 普通资源合并窗口 | 500ms |
| 继续观看合并窗口 | 1s |
| 扫描临时进度合并窗口 | 200ms |
| SSE keep-alive | 15s |

策略：

- 资源键使用集合去重。
- 扫描条目使用 `item_key` latest-wins。
- 普通扫描进度在 Dispatcher 饱和时允许丢弃。
- 扫描终态等待队列容量。
- 慢客户端不无限积压，直接要求 state resync。
- JSON 在发送前序列化一次，同权限范围订阅者共享序列化结果。
- Hub 按 public/admin/library/user scope 分发，不做全连接扇出后再过滤。

## 13. 当前架构边界

### 13.1 已满足的目标

- 最终一致性不依赖 SSE 逐条可靠投递。
- Web 和原生客户端可以使用相同资源 revision 协议。
- 继续观看高频写入已经在服务端合并。
- 扫描条目事件不会逐条直接打满每个 SSE 连接。
- 用户级和媒体库级事件不会唤醒无关连接。
- 慢客户端有明确恢复路径。
- 当前单实例 Docker 部署不需要额外 Redis、NATS 或 JetStream。

### 13.2 当前限制

1. `scan.progress` / `scan.finished` 当前通过产生扫描任务的进程内 Hub 发送；多 API/worker 实例部署时，连接到另一个实例的客户端可能看不到临时扫描进度，但最终 catalog/scan revision 仍可恢复。
2. catalog revision 当前由行级 trigger 增加，大批量扫描会多次更新同一个 aggregate revision；SSE 合并不能消除数据库侧的 revision 写放大。
3. 每个客户端仍必须完整实现资源到业务读模型的映射；映射缺失会造成局部页面保持旧缓存。Web 已将映射集中到独立模块并建立覆盖测试，原生客户端应采用同样的集中式 Revision Coordinator。

## 14. 架构审查与建议演进

以下内容说明当前 v1 已落实的架构决策和后续仍可演进的边界。

### 14.1 应保留

- 保留“持久化 revision + SSE 失效通知 + 临时扫描进度”的双通道模型。
- 保留 `/api/realtime/state`，不要改成依赖 SSE 历史回放。
- 保留扫描 progress 可丢、终态可恢复的语义。
- 近期继续使用 PostgreSQL `LISTEN/NOTIFY`，当前规模没有必要引入 NATS、JetStream 或 Redis。

### 14.2 v1 已落实的收敛

1. PostgreSQL Listener 每次订阅或重新订阅成功后，向当前实例全部连接发送 `resync.required` 并关闭连接，客户端重连后按 state 对账。
2. Web 已补齐 catalog/settings 查询映射，包括最近添加、媒体库聚合详情、搜索、演员、资源版本、播放头部、剧集大纲、音轨和字幕。
3. 一个 `resources.changed` 批次会先合并并去重 query key，再执行刷新。
4. 刷新失败会保留 dirty revision，先按退避策略重试，耗尽后回到 realtime state 对账。
5. 媒体库创建、删除使用管理员集合资源 `admin:libraries`；单库更新只增加 `library:{id}:settings`，普通用户通过自己的 library-access revision 获取集合变化。
6. `scan.finished` 携带最终 revisions，并与 `resources.changed` 共用同一个 Revision Coordinator；相同 revision 的后续通知会被忽略。
7. Realtime state、首页 baseline 和业务事件统一采用首个开发协议版本 `1`，不保留未发布格式的兼容分支。
8. SSE Hub 改为 public/admin/library/user 分域广播，避免用户级事件对全部连接产生 O(在线连接数) 的无效唤醒。
9. `library:{id}:scan` 在任务入队和持久化状态变化时递增，其他已连接客户端不需要等待第一条临时进度即可发现 pending scan。

### 14.3 数据库 revision 写放大

在大库或并行 worker 增多前，把 catalog revision 从“每一行 trigger 都 bump”改成“每个业务事务每个 aggregate 最多 bump 一次”。可以保留数据库事务原子性，但应避免同一文件 upsert 同时更新多个表时反复锁定同一 revision 行。

### 14.4 多实例临时进度

当前阶段不建议为了临时进度立即引入外部 MQ。建议先把进程内实现抽象为 `ProgressBus`：

- 单实例默认使用内存实现。
- 最终业务一致性继续依赖 PostgreSQL revision。
- 真正部署多实例时再增加 Redis、NATS 或专用 realtime gateway 适配器。

这样不会把外部基础设施变成当前用户的安装负担，同时保留后续横向扩展能力。

## 15. 客户端最小验收清单

- 能使用 cookie 或 Bearer token 建立 SSE。
- 每次首次连接、重连和前台恢复都读取 realtime state。
- 能识别 `server_epoch` 变化并丢弃旧 revision 基线。
- 能忽略重复和乱序旧 revision。
- 同一资源只有一个刷新任务。
- API 刷新失败不会错误推进 applied revision。
- `scan.progress` 丢失不会阻止最终数据出现。
- `scan.finished` 后先同步正式目录，再清理临时卡片。
- 收到 `resync.required` 后会重新对账。
- 收到 `session.invalidated` 后停止使用旧权限。
- 不会因为任意 SSE 事件全量加载全部媒体库目录。
- App 进入后台会关闭连接，回到前台会差异同步。
