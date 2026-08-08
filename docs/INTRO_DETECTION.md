# 片头检测设计

本文档定义 Mova 对剧集片头的按需检测、后台任务、输入失效、结果持久化和客户端同步规则。媒体播放接口见 [`API.md`](API.md)，资源 revision 与断线恢复见 [`SSE.md`](SSE.md)。

## 1. 目标与边界

片头检测用于找出同一季多集之间稳定重复的音频区间，并向播放器提供可选的“跳过片头”操作。

- 检测只针对本地可播放的 `episode`，以 season 为计算和复用单位。
- 播放请求只执行一次轻量、幂等的持久化任务入队，不等待 FFmpeg 或音频计算。
- SSE 不传输检测过程或最终片头对象。结果写入正式目录后推进 `library:{id}:catalog` revision，客户端重新读取播放头和 `episode-outline`。
- 播放器只提供用户主动点击的跳过操作，不自动跳过。
- 当前实现只分析音频，不做视频画面指纹、片尾检测或用户手工编辑。

## 2. 触发与任务生命周期

```text
请求 episode playback-header
  -> 读取播放头并轻量尝试入队 media.intro.detect
  -> 返回播放头
  -> background worker 领取持久化任务
  -> FFmpeg 提取代表集音频
  -> Rust 计算重复区间
  -> fencing 事务写入分析结果与 season markers
  -> 推进 library:{id}:catalog revision
  -> Web / App 定向刷新播放头和 episode-outline
```

入队需要同时满足：

- 条目类型为 `episode` 且存在本地 `season_id`。
- 当前集和所在季均没有完整、可直接使用的片头区间；升级不会覆盖已有有效 markers。
- 该季至少有 3 个包含可播放文件的本地集。
- 当前算法版本没有可复用的 `matched` 或 `no_match` 结论。
- 没有同一季的 `pending`、`running` 或 `cancel_requested` 任务。
- 上一次终态失败的 6 小时冷却已经结束。

`background_jobs` 是任务权威状态，任务类型为 `media.intro.detect`，scope 为媒体库，payload 只保存 `library_id`、`season_id` 和 `algorithm_version`，不保存媒体路径或音频内容。唯一部分索引保证多进程、多实例并发请求只能创建一个活动任务。

任务使用数据库 lease 和 execution fence。worker 每 15 秒续租，租约失效、任务被取消或服务实例失去所有权时会终止 FFmpeg 进程组，过期 worker 无法提交结果。最多尝试 3 次；全部失败后保存机器可读失败状态并进入 6 小时冷却。

## 3. 输入选择与版本语义

服务端按 `episode_number` 读取一季的本地集，每集选择一个稳定的代表文件：

1. 只选择仍归属于该集的 `media_files`。
2. 多版本时按 `created_at ASC, id ASC` 选择最早入库的文件。
3. 一季最多分析 8 集；不足 8 集时全部使用，超过时在整季首尾之间等距抽样。
4. 输入指纹覆盖整季全部代表文件，不只覆盖抽样集。指纹包含季、集号、媒体文件 ID、路径、大小、时长和 `scan_hash`。

固定上限让单季成本可预测：最多执行 8 次音频提取和 28 组两两比较。等距抽样避免只看季首几集而误判中后段更换片头的剧集。

扫描创建或替换媒体文件时会使对应季的分析记录和片头 markers 失效。新扫描入队还会取消该库待执行或正在执行的片头任务；同库扫描必须等正在运行的片头 FFmpeg 进程退出后才能被 worker 领取。结果提交前，服务端会再次读取完整输入指纹；fencing 事务会锁定 season 并校验结构化输入快照，因此过期分析不能覆盖新媒体状态。

## 4. 音频分析

每个代表文件由 FFmpeg 提取开头最多 240 秒的音频：

```text
mono / signed 16-bit PCM / 8 kHz / no video
```

每个 FFmpeg 进程只使用一个解码线程，最长运行 90 秒，整个 season 任务最长运行 10 分钟。输入协议只允许本地分析所需的 `file`、`pipe`、`crypto` 和 `data`，不会借片头分析发起网络请求。标准输出和标准错误都有硬上限；超时、取消或超限会终止整个 FFmpeg 进程组。

Rust 每秒生成一组 8 维特征：

- 对数 RMS；
- 过零率；
- 对数平均绝对振幅；
- 120、240、480、960、1920 Hz 五个归一化 Goertzel 频带能量。

特征按单集逐维标准化。两集之间只搜索开头 150 秒内、起点偏移不超过 18 秒的连续区间；逐秒余弦相似度阈值为 `0.93`，候选片头至少 12 秒、最长 150 秒。

## 5. 聚类与接受规则

两集匹配候选按起止时间各不超过 6 秒的偏差聚类。一个结果必须满足：

- 至少覆盖 3 集；
- 覆盖抽样集数的至少 60%；
- 区间不少于 12 秒；
- 综合置信度不低于 `0.82`。

综合置信度由平均相似度、集数覆盖率和区间长度组成。最终起止时间使用候选聚类的中位数，减少单集片头前广告、静音或 recap 对结果的影响。

单个文件无法解码、音轨过短或格式异常时只跳过该集。可分析集数还必须达到抽样数的 60%，且不少于 3 集；否则视为可重试的运行失败，而不是永久 `no_match`。失败集不会从置信度分母中消失，因此部分解码失败不会虚高最终置信度。

## 6. 结果与持久化

`season_intro_analyses` 保存当前算法对一季输入的权威结论：

| 字段类别 | 作用 |
| --- | --- |
| `algorithm_version` / `input_fingerprint` | 判断结果是否仍适用于当前代码和媒体输入 |
| `outcome` | `matched`、`no_match` 或 `failed` |
| markers / `confidence` | 仅 `matched` 保存起止秒数与置信度 |
| episode counts | 保存抽样、成功分析和失败集数，便于诊断 |
| `reason_code` / `retry_after` | 保存稳定原因码和失败冷却，不保存原始 FFmpeg 输出或文件路径 |

`matched` 会把同一组起止秒数写入 `seasons.intro_start_seconds` 和 `intro_end_seconds`。`no_match` 是对当前输入与算法版本的确定性结论，会清空旧的自动 markers，并在输入未变化时阻止重复分析。`failed` 表示运行环境或可分析输入不足，只在冷却期内抑制重试。

写入分析表、更新 season markers 和推进 catalog revision 位于同一数据库事务中。事务要求有效 execution fence；媒体库删除会级联删除分析状态，并取消对应的片头后台任务。

## 7. 客户端规则

- 客户端以 `episode.intro_*` 为第一优先级，缺失时使用 `season.intro_*`。
- 首次播放可能先收到空 markers，这是正常的异步语义。
- 收到 `library:{id}:catalog` 的更高 revision 后，刷新当前活动的 `media-item-playback-header` 和 `media-episode-outline` 查询。
- 当播放时间位于 `[intro_start_seconds, intro_end_seconds)` 时展示“跳过片头”；点击后 seek 到 `intro_end_seconds`。
- 客户端不得自行推断、缓存永久 no-match，或依赖收到一条专用片头 SSE 事件。

## 8. 资源与安全约束

- 全局最多执行一个 `media.intro.detect` 重型任务，避免多个 worker 或服务实例同时占满 CPU 与磁盘 IO。
- 普通扫库优先于片头任务；同库有活动扫描时不领取片头任务。
- 音频只在内存中短暂存在，不写缓存文件、不上传远端、不进入数据库。
- 持久化错误使用稳定原因码；FFmpeg 诊断只进入受控服务日志。
- 数据库迁移不要求重扫媒体库。已有有效片头区间继续使用；尚无片头区间的剧集会在下一次播放按需分析。
