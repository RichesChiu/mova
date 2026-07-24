# Mova 缓存存储与清理

本文档定义 Mova 服务端生成缓存的目录归属、生命周期、后台清理和失败恢复规则。缓存不是业务权威数据；PostgreSQL 中的媒体、用户、播放和任务数据始终是权威状态。

## 1. 目标

- 删除媒体库时完整清理该库产生的服务端缓存。
- 不扫描或修改只读媒体目录。
- 文件系统失败不回滚已经完成的数据库删除。
- 服务重启、worker 崩溃或租约过期后可以继续清理。
- 多 worker 并发时，同一库只允许一个有效清理任务。
- 运行中的扫描停止写入后，清理任务才可以领取。

## 2. 按库隔离的目录

所有可由媒体库生命周期管理的缓存必须写入：

```text
MOVA_CACHE_DIR/
└── libraries/
    └── {library_id}/
        ├── artwork/
        │   └── tmdb/
        │       ├── poster/
        │       ├── backdrop/
        │       └── logo/
        ├── subtitles/
        │   └── subtitle-{subtitle_file_id}.vtt
        └── audio-tracks/
            └── media-file-{media_file_id}-audio-track-{audio_track_id}-{version}.{container}
```

`library_id` 是数据库生成的正整数，不接受客户端路径。清理器只根据服务端缓存根目录和该整数构造目标目录，不执行 payload 中提供的任意文件路径。

TMDB 图片在单个库内按源 URL 的稳定哈希复用。不同库之间不共享物理文件，因此删库不需要猜测图片是否仍被其它库引用，也不会误删其它库的封面。

## 3. 删除事务

`DELETE /api/libraries/{id}` 在获得同库 advisory lock 后执行一个数据库事务：

1. 锁定目标 `libraries` 记录。
2. 把同库 `library.scan` 后台任务从 `pending` 置为 `cancelled`，从 `running` 置为 `cancel_requested`。
3. 执行 `DELETE FROM libraries WHERE id = $1`。
4. PostgreSQL 通过外键 `ON DELETE CASCADE` 删除库归属的媒体、季集、文件、音轨、字幕、演员、评分、外部 ID、扫描、通知、授权和播放状态。
5. 写入 `library.cache.cleanup` 后台任务，scope 固定为 `library:{id}`，最大尝试次数为 10。
6. 提交事务。

数据库删除和缓存任务入队必须同时成功或同时回滚。应用层不维护第二套手写子表删除清单。

`background_jobs.related_scan_job_id` 使用 `ON DELETE SET NULL`。删除库时扫描历史可以级联删除，但正在退出的 worker 任务记录会保留到取消完成，供缓存任务判断是否仍有写入者。

## 4. Worker 协调

缓存清理任务只有在同 scope 不存在 `running` 或 `cancel_requested` 的 `library.scan` 任务时才能领取。

运行中的扫描收到取消请求后：

- 下一次数据库租约续期会失去执行许可并设置扫描取消标记。
- worker 完成本轮安全退出后把后台任务写为 `cancelled`。
- worker 异常退出时，租约过期后数据库领取逻辑把遗留的 `cancel_requested` 任务收敛为 `cancelled`。

清理 worker 持续续租，然后对 `MOVA_CACHE_DIR/libraries/{library_id}` 执行幂等目录删除。目录不存在视为成功。删除失败时使用后台任务的延迟重试；服务重启后仍从 PostgreSQL 恢复。

## 5. 失败与通知

清理任务达到 10 次尝试后进入 `failed`，保存最后错误并写入管理员可见通知：

```text
category: system
notification_type: cache.cleanup.failed
severity: error
resource revision: admin:notifications
```

通知 payload 包含 `background_job_id`、`library_id`、删除前的 `library_name`、尝试次数和最后错误。管理员客户端收到 `admin:notifications` revision 后重新读取 `GET /api/notifications`。

## 6. 不属于缓存清理的内容

以下内容不在 `MOVA_CACHE_DIR/libraries/{library_id}` 中，删库不得修改：

- 原始电影或剧集文件。
- 媒体目录内的 NFO。
- 媒体目录内的海报、背景、Logo 或剧照。
- 外挂字幕和其它 sidecar 文件。
- PostgreSQL 数据目录。

## 7. 数据库与开发环境

`background_jobs` 的 scope、取消状态和扫描任务外键属于当前单一初始化 schema。现有开发数据库不能平滑升级，必须删除并重新初始化数据库数据，再重新扫描媒体库。

缓存布局同时改为按库隔离。重建开发环境时应删除旧缓存目录，避免保留不再受新任务管理的旧版共享缓存。
