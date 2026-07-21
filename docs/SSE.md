# Mova SSE 同步协议

本文档定义 Mova 服务端与 Web、macOS、iOS、iPadOS 客户端之间的 SSE 同步协议。协议覆盖资源版本、事件触发条件、扫描进度、断线恢复、权限失效和客户端缓存协调规则。

业务接口、请求字段和响应字段见 [`API.md`](API.md)。媒体库扫描流程见 [`MEDIA_LIBRARY_SCAN.md`](MEDIA_LIBRARY_SCAN.md)。

## 1. 协议目标

SSE 同步由两类信息组成：

1. **持久化资源版本**：表示最终业务数据是否发生变化。
2. **临时扫描状态**：表示扫描任务和扫描卡片的运行过程。

```text
业务写入事务
  -> 增加 resource revision
  -> PostgreSQL NOTIFY
  -> RealtimeDispatcher 合并通知
  -> resources.changed
  -> 客户端读取业务 API

扫描任务
  -> ScanJobEvent
  -> RealtimeDispatcher 合并进度
  -> scan.progress / scan.finished
  -> 客户端更新扫描 UI
```

协议遵循以下约束：

- SSE 不承载电影、剧集、媒体库、用户或继续观看的最终业务对象。
- 最终业务数据通过普通 HTTP API 读取。
- 客户端不依赖收到每一条事件，也不依赖临时事件完整回放。
- `realtime_revisions` 是可恢复状态；PostgreSQL `NOTIFY` 与 SSE 是低延迟唤醒信号。
- `scan.progress` 允许合并和丢失。
- Web 与原生客户端使用相同的事件、字段和恢复算法。

## 2. 协议版本

所有 SSE 业务事件和 realtime state 都包含：

```json
{"protocol_version": 1}
```

客户端必须显式支持版本 `1`。遇到不支持的版本时应停止消费事件、关闭连接并提示升级，不得猜测字段语义。

## 3. 接口

### 3.1 `GET /api/realtime/events`

建立 SSE 长连接。

- 响应类型：`text/event-stream`
- Web 鉴权：session cookie
- 原生客户端鉴权：`Authorization: Bearer <access_token>`
- refresh token 不得用于建立 SSE 连接
- 服务端每 15 秒发送 keep-alive
- keep-alive 不包含业务状态
- 协议不提供 SSE `id`
- 协议不支持 `Last-Event-ID` 历史回放

连接只接收建立连接之后产生的事件。首次连接、重连和 App 回到前台时，客户端必须先调用 realtime state 完成差异同步。

### 3.2 `GET /api/realtime/state`

返回登录用户有权访问的持久化资源版本和活跃扫描任务。接口使用普通 API envelope，`data` 示例：

```json
{
  "protocol_version": 1,
  "server_epoch": "019f...",
  "resources": {
    "admin:libraries": 14,
    "library:7:settings": 3,
    "library:7:catalog": 128,
    "library:7:scan": 9,
    "library:7:notifications": 5,
    "user:12:continue-watching": 39,
    "user:12:notifications": 8,
    "user:12:profile": 2
  },
  "active_scans": []
}
```

字段语义：

- `protocol_version`：SSE 协议版本。
- `server_epoch`：数据库生命周期标识。服务进程重启不改变该值，数据库重建会生成新值。
- `resources`：登录用户可见资源的最高 revision；未发生过变化的资源返回 `0`。
- `active_scans`：状态为 `pending` 或 `running` 的扫描任务，包含持久化任务进度，不包含历史扫描卡片事件。

客户端检测到 `server_epoch` 变化时，必须清除本地 revision 基线并重新读取所需业务数据。

### 3.3 `GET /api/home` 的同步基线

首页响应包含：

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

该字段只表示首页读模型已经应用的 revision。媒体详情、用户管理和其他独立读模型仍按各自业务 API 建立缓存基线。

## 4. 资源键

资源键表示需要重新读取的业务聚合，不表示单个数据库行。

| 资源键 | 触发条件 | 可见范围 | 客户端读取内容 |
| --- | --- | --- | --- |
| `admin:libraries` | 媒体库插入或删除 | 管理员 | 管理员媒体库集合、首页库摘要 |
| `library:{id}:settings` | 指定媒体库更新或删除 | 有权访问该库的用户 | 媒体库详情、列表摘要、首页库摘要 |
| `library:{id}:catalog` | 媒体条目、媒体文件、外部身份、评分、季或集发生写入 | 有权访问该库的用户 | 库目录、最近添加、首页预览、媒体详情、演员、资源版本、播放头部、剧集大纲 |
| `library:{id}:scan` | 扫描任务创建或状态改变 | 有权访问该库的用户 | 扫描任务、库详情、realtime active scans |
| `library:{id}:notifications` | 该库产生一条新通知或既有通知内容更新 | 有权访问该库的用户 | 通用通知中心 |
| `user:{id}:libraries` | 用户媒体库权限改变 | 指定用户 | 可见媒体库列表、首页 |
| `user:{id}:profile` | 用户资料改变 | 指定用户 | 用户资料及引用用户摘要的页面 |
| `user:{id}:continue-watching` | 继续观看队列改变 | 指定用户 | 继续观看列表、首页继续观看区域 |
| `user:{id}:notifications` | 指定用户的通知或已读状态改变 | 指定用户 | 通用通知中心和未读数 |
| `admin:users` | 用户插入、更新或删除 | 管理员 | 用户管理列表 |

通知不是独立 SSE 事件。服务端把通知正文和每个用户的已读状态持久化后只推进对应 `notifications` revision；客户端收到失效信号后调用 `GET /api/notifications`。因此断线期间产生的通知不会丢失，重复或乱序的 revision 也不会重复创建通知。

内部资源键 `session:user:{id}` 不出现在 state 和 `resources.changed` 中。以下操作触发该键，并转换为 `session.invalidated`：

- 用户被删除。
- 用户角色改变。
- 用户启用状态改变。
- 密码哈希改变。
- 用户媒体库权限改变。

## 5. 事件总览

| 事件 | 数据性质 | 投递语义 | 客户端职责 |
| --- | --- | --- | --- |
| `resources.changed` | 资源键与 revision | 通知可丢，revision 可恢复 | 定向读取业务 API |
| `scan.progress` | 临时扫描任务和条目状态 | 普通批次允许丢失；检查点走可靠进程内 FIFO | 展示扫描进度和临时卡片 |
| `scan.finished` | 扫描终态和最终 revisions | 立即发送；仍以 revision 恢复为兜底 | 刷新正式目录后移除临时卡片 |
| `resync.required` | 重同步原因 | 发送后关闭连接 | 读取 state 并重新连接 |
| `session.invalidated` | 权限失效原因 | 发送后关闭连接 | 重新建立登录态 |

## 6. `resources.changed`

### 6.1 触发流程

1. 业务 mutation 在数据库事务中写入数据。
2. 同一事务增加 `realtime_revisions.revision`。
3. 数据库调用 `pg_notify('mova_realtime', resource_key)`。
4. 事务提交后 PostgreSQL Listener 收到通知。
5. `RealtimeDispatcher` 按资源键去重并读取最高 revision。
6. Dispatcher 按权限范围发送事件。

业务事务回滚时，数据、revision 和通知一起回滚。

扫描组事务通过 `mova.defer_catalog_revision` 延迟逐行 catalog trigger，并在组事务末尾显式增加一次 catalog revision。

### 6.2 合并频率

- 普通资源：最多每 500ms 发送一批。
- 继续观看：最多每 1 秒发送一批。
- 标记已看完：立即发送继续观看 revision。
- 同一窗口内的同一资源只发送最高 revision。

### 6.3 Payload

```text
event: resources.changed
data: {
  "protocol_version": 1,
  "changes": [
    {"resource": "library:7:catalog", "revision": 128},
    {"resource": "user:12:continue-watching", "revision": 39}
  ]
}
```

不同权限范围可以形成多个事件批次。客户端不得假设一个事件包含某一时刻的全部资源。

### 6.4 Revision Coordinator

客户端为每个资源维护：

- `applied_revision`：相关读模型已经刷新成功或可靠标记为 stale 的版本。
- `requested_revision`：收到但尚未完成同步的最高版本。
- `in_flight`：该资源是否正在刷新。

处理算法：

1. `revision <= applied_revision`：忽略。
2. 更新 `requested_revision = max(requested_revision, revision)`。
3. `in_flight = true`：不启动重复请求。
4. 未刷新时按资源映射读取业务 API。
5. 刷新期间收到更高 revision，当前请求完成后继续同步至最高版本。
6. 请求失败时保留 dirty revision，按退避策略重试；重试耗尽后读取 realtime state 对账。

未加载的页面只需将对应资源标记为 dirty，在用户进入页面时读取业务 API。客户端不得把未刷新的缓存标记为已经应用新 revision。

## 7. `scan.progress`

### 7.1 触发条件

- 扫描任务启动或 phase 改变。
- 文件发现数量达到节流阈值。
- 扫描组完成本地分析。
- 扫描组完成 pending 短事务。
- 所有待处理组完成 pending 提交，形成本地检查点。
- 扫描组进入 metadata 阶段。
- 扫描组进入 artwork 阶段。
- 扫描组完成最终入库。
- 扫描尝试失败且进入等待重试状态。

### 7.2 合并规则

- 普通进度最多每 200ms 发送一批。
- `scan_job` 使用 latest-wins。
- 相同 `(scan_job_id, item_key)` 使用 latest-wins。
- Dispatcher 输入队列饱和时允许丢弃普通进度。
- 本地检查点与 `scan.finished` 使用独立的进程内 FIFO。
- 扫描事件共享单调序号。
- 终态屏障保留 60 秒，忽略序号小于或等于终态序号的晚到事件。
- 合法重试产生更大的序号，可以继续使用同一个 scan job。

这里的可靠 FIFO 只保证服务进程存活期间不因普通队列饱和而丢弃检查点或终态。进程退出后的恢复依赖 revisions 与 realtime state。

### 7.3 Payload

```text
event: scan.progress
data: {
  "protocol_version": 1,
  "scan_job": {
    "id": 41,
    "library_id": 7,
    "status": "running",
    "phase": "processing",
    "total_files": 240,
    "scanned_files": 240,
    "local_analyzed_files": 52,
    "local_committed_files": 48,
    "remote_completed_files": 20,
    "progress_percent": 22,
    "created_at": "2026-07-14T00:00:00Z",
    "started_at": "2026-07-14T00:00:01Z",
    "finished_at": null,
    "error_message": null
  },
  "items": [
    {
      "scan_job_id": 41,
      "library_id": 7,
      "item_key": "series-folder:arcane",
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
      "progress_percent": 85
    }
  ],
  "changes": []
}
```

本地检查点的 `changes` 包含 `library:{id}:catalog` 与 `library:{id}:scan`；普通进度的 `changes` 为空数组。

### 7.4 扫描任务状态

扫描任务 phase：

- `null`：任务等待 worker 或等待后台重试。
- `discovering`：发现文件、建立增量计划和浅层分组。
- `processing`：local worker 与 remote worker 有界重叠运行。
- `finalizing`：执行缺失路径对齐与任务收口。
- `finished`：任务处于终态。

任务进度由服务端持久化：

| 状态 | 进度 | 含义 |
| --- | ---: | --- |
| 首次 `pending` | 0 | 任务等待 worker |
| 重试 `pending` | 保留最后值 | 任务等待下一次执行，`error_message` 保存失败上下文 |
| `running / discovering` | 1～10 | 发现文件与建立计划 |
| `running / processing` | 10～99 | 本地分析、pending 提交和远端处理 |
| `running / finalizing` | 99 | 收敛缺失路径和最终状态 |
| `success / finished` | 100 | 任务成功完成 |
| `failed / finished` | 保留最后值 | 任务失败或取消 |

任务进度公式：

```text
progress = floor(
  10
  + 20 * local_analyzed_files / total_files
  + 20 * local_committed_files / total_files
  + 49 * remote_completed_files / total_files
)
```

运行中最大为 99。`matched`、`unmatched`、`failed` 和 `skipped` 都表示扫描组已经完成本次远端处理，因此计入任务完成度。客户端必须直接展示 `scan_job.progress_percent`，不得根据 phase、事件数量或条目进度重新计算。

### 7.5 扫描条目状态

| stage | 展示百分比 | 含义 |
| --- | ---: | --- |
| `analyzed` | 30 | 完成 sidecar、`ffprobe` 和技术信息分析 |
| `pending_committed` | 40 | pending 短事务已经提交 |
| `metadata` | 60 | 正在获取或判断远端元数据 |
| `artwork` | 85 | 正在获取海报与背景图 |
| `completed` | 100 | 最终组事务已经提交 |

条目百分比仅用于单个临时卡片动画，不参与任务总进度计算。

客户端按 `library_id` 保存扫描运行时，按 `item_key` 合并条目；出现新的 `scan_job.id` 时删除该库上一个任务的临时条目。每个媒体库最多保留 40 个临时扫描条目。

## 8. `scan.finished`

任务成功、取消或重试额度耗尽时发送。仍有重试额度的失败尝试只恢复为 `pending`，不发送终态事件。

扫描终态会在同一数据库事务中写入扫描任务、worker 累计的扫描摘要和一条 `scan` 类通知，并推进 `library:{id}:scan` 与 `library:{id}:notifications`。客户端收到 `scan.finished` 后刷新正式目录和通知中心。`scan.finished` 不直接携带通知正文或完整错误列表，断线重连也不需要回放扫描日志；客户端始终通过 `GET /api/notifications` 读取持久化摘要。

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
    "scanned_files": 240,
    "local_analyzed_files": 240,
    "local_committed_files": 240,
    "remote_completed_files": 240,
    "progress_percent": 100
  },
  "items": [],
  "changes": [
    {"resource": "library:7:catalog", "revision": 128},
    {"resource": "library:7:notifications", "revision": 5},
    {"resource": "library:7:scan", "revision": 9}
  ]
}
```

客户端处理顺序：

1. 显示最终任务状态。
2. 将 `changes` 交给处理 `resources.changed` 的 Revision Coordinator。
3. 刷新正式目录和扫描状态。
4. 正式数据刷新成功后移除临时扫描卡片。
5. `changes` 为空或刷新失败时保留临时状态，并读取 realtime state 对账。

终态事件丢失时，客户端通过 `library:{id}:scan` revision、`library:{id}:catalog` revision 和 `active_scans` 恢复。

## 9. `resync.required`

```text
event: resync.required
data: {"protocol_version":1,"reason":"client_lagged"}
```

触发条件：

- 客户端落后于有界广播队列。
- PostgreSQL Listener 建立或重新建立订阅，需要关闭潜在通知空档。

服务端发送事件后关闭连接。客户端必须：

1. 停止处理该连接。
2. 读取 `/api/realtime/state`。
3. 按 revision 刷新 dirty 资源。
4. 建立新的 SSE 连接。

## 10. `session.invalidated`

```text
event: session.invalidated
data: {"protocol_version":1,"reason":"authorization_changed"}
```

服务端发送事件后关闭连接。客户端必须停止使用连接建立时的权限快照，并重新验证登录态。

- Web：重新验证 session cookie。
- 原生客户端：使用 access token 获取用户资料；收到 `401` 时调用 refresh 接口轮换 token。
- 验证失败：清除登录态并进入登录页面。

## 11. 连接生命周期

### 11.1 Web

1. 登录成功后读取首页或 realtime state。
2. 建立 revision 基线。
3. 建立 SSE 连接。
4. `open` 后安排一次 state 对账，覆盖连接建立窗口中的竞态。
5. 网络断开后按退避策略重连。

### 11.2 macOS

- 前台保持 SSE 连接。
- 网络切换或唤醒后读取 state，再重新连接。
- Revision Coordinator 放在共享 Core 层，不与具体 SwiftUI 页面绑定。

### 11.3 iOS / iPadOS

- 进入后台时关闭 SSE。
- 回到前台时读取 state。
- 完成差异同步后建立 SSE 连接。
- 不依赖后台长连接持续存活。

## 12. 权限与分发范围

事件按以下 scope 分发：

- `Public`：所有已登录连接。
- `Admin`：管理员连接。
- `Library(id)`：管理员和拥有该库权限的用户。
- `User(id)`：指定用户。

Hub 按 scope 使用独立广播频道。用户级或单库事件不会唤醒无关连接。管理员订阅管理员频道并接收所有 library scope 事件。

业务 API 必须独立执行鉴权。收到 SSE 事件不构成访问资源的授权。

## 13. 限流与背压参数

| 项目 | 参数 |
| --- | ---: |
| Dispatcher 命令队列 | 2048 条命令 |
| 每个 scope 的 SSE 广播队列 | 32 个批次 |
| 普通资源合并窗口 | 500ms |
| 继续观看合并窗口 | 1s |
| 扫描进度合并窗口 | 200ms |
| SSE keep-alive | 15s |
| 扫描终态屏障 | 60s |

背压规则：

- 普通扫描进度允许丢弃。
- 检查点和扫描终态使用独立 FIFO。
- 业务资源 revision 持久化在 PostgreSQL。
- 慢客户端收到 `resync.required` 后断开并通过 state 恢复。
- 同一 payload 预序列化后由同一权限范围的连接共享。

## 14. 部署约束

单实例部署使用 PostgreSQL `LISTEN/NOTIFY`、进程内 Dispatcher 和分域 Hub，不依赖 Redis、NATS 或 JetStream。

多实例部署必须满足：

- resource revision 继续以 PostgreSQL 为权威状态。
- 各实例通过 PostgreSQL `LISTEN/NOTIFY` 接收资源失效通知。
- 临时扫描进度需要跨实例时，应为 Dispatcher 提供可替换的 `ProgressBus`。
- API 实例、worker 实例与 realtime 实例共享相同的 scan job、resource revision 和权限模型。
- 外部消息组件只承载临时事件分发，不替代业务数据库和 revision 恢复机制。

## 15. 客户端验收清单

- 支持 `protocol_version = 1`。
- 检测 `server_epoch` 变化并清除 revision 基线。
- 忽略重复或乱序的较低 revision。
- 相同资源只允许一个刷新任务。
- 刷新失败时保留 dirty revision。
- SSE 断线后通过 state 恢复。
- App 回到前台时先读取 state。
- 扫描任务进度只使用服务端 `progress_percent`。
- 扫描条目按 `item_key` latest-wins 合并。
- `scan.finished` 后先刷新正式数据，再移除临时卡片。
- 收到 `resync.required` 后对账并重连。
- 收到 `session.invalidated` 后重新验证登录态。
- 权限判断始终由业务 API 负责。
