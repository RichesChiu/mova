# 媒体库扫描性能验证

本文档记录扫描流水线容量选型的可重复证据、真实环境样本和验收边界。扫描业务契约见
[`MEDIA_LIBRARY_SCAN.md`](MEDIA_LIBRARY_SCAN.md)。本文不是墙钟时间承诺；网络质量、媒体文件、
磁盘、TMDB 响应和设备性能都会影响绝对耗时。

## 结论

生产默认值为：

- 远端元数据与图片准备并发：`4`
- local→remote 队列容量：`2`
- remote→commit 队列容量：`2`
- 数据库提交、图片发布完成、权威任务进度与完成事件：单协调器串行执行

选择 4 而不是 8，是因为 4 已在确定性模型中取得明显收益，并保留 TMDB、图片带宽、内存和
数据库连接余量。8 只证明模型上仍可能更快，尚不足以成为多用户部署的保守默认值。队列保持
为 2，是因为扩大到 16 没有改善模型总耗时，反而增加组等待延迟和在途内存。

## 可重复容量模型

测试位于
[`crates/mova-application/src/scan_jobs/performance_tests.rs`](../crates/mova-application/src/scan_jobs/performance_tests.rs)，
使用 120 个电影/剧集混合组、12 秒本地分析总工作量、276 次 TMDB 请求、288 次图片请求，
并模拟进程级 TMDB 请求起始间隔与串行数据库提交。它不等待真实墙钟，因此 CI 中结果稳定。

```bash
cargo test -p mova-application \
  selected_remote_pipeline_configuration_is_evidence_backed -- --nocapture
```

| 远端并发 | 队列 | 模拟总耗时 | 相对串行 | p95 组延迟 | 吞吐（组/分钟） |
| ---: | ---: | ---: | ---: | ---: | ---: |
| 1 | 2 | 86.13s | 1.00× | 2.955s | 83.59 |
| 2 | 4 | 43.59s | 1.98× | 2.785s | 165.18 |
| **4** | **2** | **22.53s** | **3.82×** | **1.565s** | **319.57** |
| 4 | 16 | 22.53s | 3.82× | 4.035s | 319.57 |
| 8 | 2 | 13.17s | 6.54× | 1.185s | 546.70 |

自动断言覆盖：并发增加时模型总耗时单调下降；4 并发下扩大队列不改善总耗时且恶化 p95；
生产常量实际为 4/2；模型最大在途远端任务不超过配置值。

## 真实扫描基线

2026-08-12 在本地 Docker source stack、真实 TMDB token 与真实图片下载链路上记录了
串行基线，并在启用 4/2 后使用相同的 107 文件树、新媒体库和冷库级图片目录再次扫描：

| 样本 | 文件 | 扫描组 | 总耗时 | 本地阶段 | 远端阶段 | finalize I/O | finalize DB |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 剧集目录，串行 | 63 | 1 | 34.953s | 2.248s | 34.876s | 17ms | 3ms |
| 电影目录，串行 | 107 | 107 | 139.718s | 137.668s | 139.559s | 29ms | 3ms |
| 电影目录，4/2 | 107 | 107 | 185.483s | 179.776s | 185.357s | 22ms | 2ms |

两次电影扫描都完整提交 107 个组；4/2 扫描的权威计数最终为
`local_analyzed=107 / local_committed=107 / remote_completed=107 / progress=100`，且没有
TMDB provider error 或 `429`。串行样本中 105 个文件匹配、2 个未匹配。其
106/107 个媒体文件小于 1 MiB，且包含大量不能被 ffprobe 正常解析的占位文件，所以该样本
适合证明“远端消费者和队列背压控制总墙钟”，不代表真实大文件的 ffprobe 成本。真实设备的
回归比较必须使用相同文件树、冷暖缓存状态和近似网络条件。

这组真实 A/B 没有证明墙钟提速：4/2 的第二次扫描反而慢 32.8%。两次请求发生在不同时间窗，
无法控制代理、TMDB 和图片 CDN 延迟；同时当前 `local_pipeline_ms` 包含向下游队列等待，并非
纯 CPU/ffprobe 计时。因此报告不会把 3.82× 模型收益写成真实环境收益。4/2 的依据是确定性
容量模型、正确性测试和保守资源上限；后续只有在同时间窗、可控 mock 上游或多轮交替测试中
观察到稳定结果，才据此调整默认并发。

## 安全与正确性断言

- `buffer_unordered(4)` 只并行远端准备；数据库写入由一个协调器处理。
- 两段容量为 2 的 channel 限制已分析组和已下载组的在途数量。
- TMDB provider 仍共享进程级请求起始限速、重试和 `Retry-After` 策略。
- 克隆的 enrichment context 共享有界缓存；同一个请求键使用 single-flight 锁，四个 worker
  同时 miss 时只调用一次 provider。
- 图片发布 guard 从下载前持有到引用事务提交并释放；每个 guard 使用独立数据库连接，不占
  应用连接池，取消或错误会通过 drop/finish 路径释放并清理未引用图片。
- `remote_completed_files` 和 `progress_percent` 在同一个组事务中原子递增，进度不会因为
  完成顺序不同而回退。

相关自动测试：

```bash
cargo test -p mova-application \
  cloned_contexts_collapse_concurrent_identical_provider_requests
cargo test -p mova-application
cargo test --workspace --locked -- --include-ignored --test-threads=1
```

2026-08-12 使用临时 PostgreSQL 18 执行最后一条命令，应用、数据库、领域、扫描和服务端共
`649` 个测试全部通过，`0` 失败、`0` 忽略。它覆盖事务进度、任务 fencing、图片发布 guard、
扫描取消、缓存 single-flight、STRM、本地字幕索引和上述容量模型。没有 `DATABASE_URL` 时，
需要 Postgres 的用例会明确显示为 ignored，而不是伪装成通过。
