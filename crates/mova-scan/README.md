# mova-scan

`mova-scan` 是 Mova 的文件系统扫描能力 crate。  
它负责从磁盘发现媒体文件、解析文件名和目录信息、读取 sidecar、探测媒体流信息，以及补出扫描阶段可直接使用的结构化结果。

## 1. 这个 crate 在系统里的位置

调用关系通常是：

`mova-application::scan_jobs` / `mova-application::file_sync` -> `mova-scan`

它的职责是：

- 递归发现媒体文件
- 识别电影/剧集线索
- 尽量把剧集目录先归成一个扫描组，而不是先把每一集都当成独立展示单位
- 读取 `.nfo` / `poster` / `fanart` 等 sidecar
- 通过 `ffprobe` 补充媒体技术信息
- 发现内嵌音轨、外挂或内嵌字幕轨道
- 在远端元数据不可用时，尽量只靠本地命名规则把剧集先聚合起来

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
| `src/parse.rs` | 从文件名/路径中识别电影或剧集信号，例如 `S01E02`、`EP02`、`Episode 03`、`第03集`，以及 `Season 01/01.mkv` 这类依赖目录信息的命名；当文件名很脏时也会优先参考更干净的父目录标题，但会避开 `合集 / collection / box set` 这类合集目录。 |
| `src/sidecar.rs` | 读取 `.nfo`、海报、背景图等 sidecar 资产。 |
| `src/probe.rs` | 调用 `ffprobe`，补时长、编码、分辨率和码率。 |
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
3. 对每个文件做文件名解析、sidecar 读取、探测
4. 对剧集路径优先从目录名里推导“系列展示名 / 远端匹配标题 / 年份”这类组级信息
5. 返回 `DiscoveredMediaFile`
6. 应用层再决定如何补元数据、如何写库

另一个使用点是 watcher/reconcile：

- watcher 发现某些路径有变更
- 应用层调用这里做局部路径扫描
- 再把结果交给 `mova-db` 做路径级同步

## 7. 当前适合继续放什么

适合继续放在这里的：

- 文件发现
- 文件名/目录结构解析
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
