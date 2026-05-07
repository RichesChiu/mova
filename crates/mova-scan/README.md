# mova-scan

`mova-scan` 是 Mova 的文件系统扫描能力 crate。  
它负责从磁盘发现媒体文件、解析文件名、读取 sidecar、探测媒体流信息，以及补出扫描阶段可直接使用的结构化结果。

## 1. 这个 crate 在系统里的位置

调用关系通常是：

`mova-application::scan_jobs` / `mova-application::file_sync` -> `mova-scan`

它的职责是：

- 递归发现媒体文件
- 只根据视频文件名识别电影/剧集线索
- 文件名没有明确剧名和季集号时，保留为本地文件条目，不递归猜目录名
- 读取 `.nfo` / `poster` / `fanart` 等 sidecar
- 通过 `ffprobe` 补充媒体技术信息
- 发现内嵌音轨、外挂或内嵌字幕轨道
- 在远端元数据不可用时，尽量只靠本地命名规则把剧集先聚合起来；剧集文件名里的年份只作为元数据线索，不作为多季聚合身份

它不负责：

- 媒体库业务规则
- 数据库写入
- HTTP 协议

## 2. 入口文件

| 文件 | 作用 |
| --- | --- |
| `src/lib.rs` | crate 入口，导出扫描 API 与 `DiscoveredMediaFile`、`DiscoveredAudioTrack`、`DiscoveredSubtitleTrack` 结构。 |

## 3. 当前模块

| 文件 | 作用 |
| --- | --- |
| `src/discover.rs` | 递归发现媒体文件、路径级扫描、支持进度回调和取消信号。 |
| `src/parse.rs` | 从文件名中识别电影或剧集信号，例如 `剧名.S01E02.mkv`、`剧名 - S01E02.mkv`、`剧名_S01E02.mkv`、`剧名-S01E02.mkv`、`剧名.1x02.mkv`。文件名必须同时提供剧名和季集号才会按剧集归组；`S01E02.mkv`、`01.mkv`、`EP02.mkv`、`第03集.mkv` 这类缺少剧名的文件不会回退目录名，`The.BeautyS01E01.mkv` 这类标题和季集号没有分隔符的脏命名也不会强行拆分，会作为本地文件名条目保留。 |
| `src/sidecar.rs` | 读取 `.nfo`、海报、背景图等 sidecar 资产。 |
| `src/probe.rs` | 调用 `ffprobe`，补时长、编码、分辨率、码率，并归一化 `HDR10`、`HDR10+`、`Dolby Vision`、`HLG`、`DTS`、`DTS-HD`、`Atmos` 等资源技术标签。 |
| `src/subtitle.rs` | 字幕轨道相关发现与归一化。 |
| `src/tests.rs` | crate 级扫描测试。 |

## 4. 主要导出能力

当前 `lib.rs` 主要导出：

- `discover_media_files`
- `discover_media_files_with_progress`
- `discover_media_files_with_progress_and_cancel`
- `discover_media_files_with_progress_item_and_cancel`
- `discover_media_paths`
- `inspect_media_file`
- `infer_series_file_metadata`
- `is_likely_episode_path`
- `DiscoveredMediaFile`
- `DiscoveredAudioTrack`
- `DiscoveredSubtitleTrack`

## 5. 关键数据结构

### `DiscoveredMediaFile`

这是扫描阶段最重要的输出结构，里面已经包含：

- 文件路径
- 标题、原始标题、排序标题
- 年份
- 季号、集号、季标题
- 简介
- 海报/背景图路径
- 文件大小、容器、时长、编码、分辨率、码率
- 从 `ffprobe` 探测结果归一化出的资源技术标签
- 内嵌音轨列表
- 字幕轨道列表

也就是说，应用层在真正写库前，先拿到的是一份“尽量丰富但还没入库”的媒体快照。

### `DiscoveredAudioTrack`

用于表达：

- 内嵌音轨的 `stream_index`
- 语言、codec、标题
- 是否是默认音轨

### `DiscoveredSubtitleTrack`

用于表达：

- 字幕来源是 external 还是 embedded
- 外挂字幕路径
- 内嵌字幕 stream index
- 语言、格式、标签、default/forced 等属性

## 6. 典型使用方式

最典型的使用点是扫描任务：

1. `mova-application` 调用 `discover_media_files_with_progress_*`
2. `mova-scan` 递归发现媒体文件
3. 对每个文件做文件名解析、sidecar 读取、`ffprobe` 探测和技术标签归一化
4. 对剧集路径只从文件名里的 `剧名.S01E01`、`剧名 - S01E01`、`剧名_S01E01` 这类显式标题推导“系列展示名 / 远端匹配标题 / 年份”，无显式标题时保留为本地文件条目；应用层会按剧名或明确季目录树聚合多季，避免每季不同年份、或同一剧集容器内不同语言文件名拆成多个剧集
5. 返回 `DiscoveredMediaFile`
6. 应用层再决定如何补元数据、如何写库

另一个使用点是手动扫描后的路径对齐：

- 应用层在显式扫库时调用这里做文件发现与探测
- 某些需要按路径重建结果的同步流程也会复用这里
- 再把结果交给 `mova-db` 做路径级同步

## 7. 当前适合继续放什么

适合继续放在这里的：

- 文件发现
- 文件名解析
- sidecar 读取
- `ffprobe` / 字幕探测

不适合继续放在这里的：

- TMDB 查询
- 媒体库业务规则
- 持久层 upsert
- 扫描任务状态管理

如果要看这个 crate 的输出最终怎样落库：

- 应用层：[`../mova-application/README.md`](../mova-application/README.md)
- 持久层：[`../mova-db/README.md`](../mova-db/README.md)
