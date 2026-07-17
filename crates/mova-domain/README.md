# mova-domain

`mova-domain` 是 Mova 的共享领域模型 crate。  
它只放纯数据结构和少量与结构绑定的简单逻辑，不包含 SQL、HTTP 或扫描 IO。

## 1. 这个 crate 在系统里的位置

它被整个 workspace 广泛复用：

- `mova-db` 用它作为数据库结果的映射目标
- `mova-application` 用它作为业务输入输出中的核心对象
- `mova-server` 用它做鉴权和访问控制

这个 crate 的主要价值是：

- 让不同层使用同一套核心模型
- 减少重复定义结构体
- 保持“数据库对象”“业务对象”“权限对象”在命名上统一

## 2. 入口文件

| 文件 | 作用 |
| --- | --- |
| `src/lib.rs` | crate 入口，统一导出所有领域对象和枚举。 |

## 3. 当前类型文件

| 文件 | 作用 |
| --- | --- |
| `src/library.rs` | `Library` |
| `src/library_detail.rs` | `LibraryDetail` |
| `src/media_item.rs` | `MediaItem` |
| `src/media_file.rs` | `MediaFile` |
| `src/audio_track.rs` | `AudioTrack` |
| `src/subtitle_file.rs` | `SubtitleFile` |
| `src/season.rs` | `Season` |
| `src/episode.rs` | `Episode` |
| `src/scan_job.rs` | `ScanJob` 扫描任务状态。 |
| `src/notification.rs` | 通用 `Notification`、包含分类未读计数的 `NotificationFeed`，以及扫描终态通知使用的摘要和问题类型。 |
| `src/playback_progress.rs` | `PlaybackProgress` |
| `src/continue_watching_item.rs` | `ContinueWatchingItem` |
| `src/media_cast_member.rs` | `MediaCastMember` |
| `src/user.rs` | `User`、`UserRole`，其中 `User` 会同时承载登录用户名和用于前端展示的昵称。 |
| `src/user_profile.rs` | `UserProfile`，包含访问控制所需的用户上下文能力。 |

## 4. 主要导出对象

当前 `lib.rs` 统一导出：

- `Library`
- `LibraryDetail`
- `MediaItem`
- `MediaFile`
- `AudioTrack`
- `SubtitleFile`
- `Season`
- `Episode`
- `ScanJob`
- `ScanNotificationSummary`
- `ScanNotificationIssue`
- `Notification`
- `NotificationFeed`
- `PlaybackProgress`
- `ContinueWatchingItem`
- `MediaCastMember`
- `User`
- `UserRole`
- `UserProfile`

## 5. 这个 crate 适合放什么

适合继续放在这里的：

- 纯领域对象
- 角色枚举
- 与领域对象强绑定、但不涉及 IO 的小型 helper

不适合放在这里的：

- 数据库查询
- HTTP response DTO
- Axum request/response 结构
- 扫描、TMDB、文件系统相关逻辑

## 6. 当前最关键的几个对象

- `UserProfile`
  - 服务端 `auth.rs` 会用它来判断是否是管理员、是否能访问某个媒体库。

- `Library` / `LibraryDetail`
  - 媒体库列表、详情、设置页和扫描链路都围绕这两个对象展开；库对象不再包含启用/禁用状态。

- `MediaItem` / `MediaFile` / `AudioTrack`
  - 一个负责逻辑上的媒体条目，一个负责物理文件与播放链路，`AudioTrack` 负责补齐单个媒体文件的内嵌音轨建模。`MediaItem` 同时暴露 `metadata_status`、`metadata_failure_reason` 和 `remote_media_type`，让前端明确区分待远端确认、已匹配、待复核、失败和跳过，而不是用 TMDB ID 是否为空做隐式判断；`pending` 只用于扫描中的中间写库，完成后必须收敛到 `matched` / `unmatched` / `failed` / `skipped`。

- `PlaybackProgress` / `ContinueWatchingItem`
  - 前者表达逐文件当前进度，后者表达有上限的活跃 Continue 队列中聚合后的电影或 Series 卡片。

如果要看这些模型被怎样读写：

- 持久层：[`../mova-db/README.md`](../mova-db/README.md)
- 应用层：[`../mova-application/README.md`](../mova-application/README.md)
