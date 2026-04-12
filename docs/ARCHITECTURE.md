# Mova 结构整理建议

这份文档不描述业务功能，而是整理当前项目结构是否合理、哪些地方已经稳定、哪些地方开始变重、以及后续应该按什么方向拆分，避免代码继续增长后变得难维护。

## 1. 当前判断

当前 workspace 的 crate 边界总体是合理的，不需要急着继续拆 crate：

- `apps/mova-web`
  - React + Vite 前端原型、路由、查询层、管理界面
- `apps/mova-server`
  - HTTP 入口、Axum 路由、应用启动、进程内扫描/runtime
- `crates/mova-application`
  - 应用层用例、扫描编排、元数据补全、播放进度业务
- `crates/mova-db`
  - SQL 和持久化读写
- `crates/mova-domain`
  - 共享领域对象
- `crates/mova-scan`
  - 文件系统扫描、文件名解析、sidecar 读取、`ffprobe` 探测

也就是说，当前更大的问题不是 “crate 太少”，而是 “crate 内部某些文件已经太重，职责开始混在一起”。

## 2. 目前值得保留的部分

这些结构目前是合理的，建议继续保留：

- 使用 `apps/` + `crates/` 的 workspace 形态
- `mova-web` 保持独立前端应用，不和 `mova-server` 强行并成全栈单体
- `mova-server -> mova-application -> mova-db` 这条依赖方向
- `mova-scan` 作为独立扫描能力，而不是把文件发现和 `ffprobe` 逻辑直接塞回 server
- 运行时数据集中到 `data/`
- 开发样例媒体集中到 `media/`

当前阶段不建议：

- 为前端再单独拆 UI component library
- 为扫描 runtime 单独再拆一个 crate
- 为 TMDB 单独再拆一个 crate
- 为 HTTP response DTO 再拆一个 crate

这些还没重到值得增加 workspace 复杂度。现在优先级更高的是先拆大文件。

## 3. 已完成的结构收敛

这两步已经落地：

- `crates/mova-db/src/media_items.rs`
  - 已经拆成：
    - `media_items.rs` 作为父模块和公共类型入口
    - `media_items/query.rs`
    - `media_items/sync.rs`
    - `media_items/series.rs`
- `crates/mova-scan/src/lib.rs`
  - 已经拆成：
    - `lib.rs` 作为对外入口
    - `discover.rs`
    - `parse.rs`
    - `sidecar.rs`
    - `probe.rs`
    - `tests.rs`

这说明当前这条重构路线是可执行的，而且不会强迫我们改业务语义。

## 4. 当前最重的热点文件

按当前代码体量，以下文件已经明显承担了过多职责：

- `crates/mova-application/src/metadata.rs`
  - 同时负责 provider 抽象、TMDB 请求、匹配规则、标题归一化、结果合并
- `crates/mova-application/src/scan_jobs.rs`
  - 同时负责任务执行编排、扫描进度推进、元数据补全接入、媒体条目构建
- `apps/mova-server/src/sync_runtime.rs`
  - 当前主要负责后台扫描入队、任务执行和实时事件桥接
- `apps/mova-server/src/response.rs`
  - 所有 feature 的 response mapping 都堆在一个文件里
- `apps/mova-server/src/state.rs`
  - 同时负责扫描注册表、删除流程状态和共享运行时依赖

这些文件继续膨胀的话，后面会出现几个典型问题：

- 改一个功能会碰到多个不相关职责
- 测试粒度变粗，局部改动难验证
- 新增 feature 时很难知道应该落在哪个文件
- 同类逻辑散在不同层里，后面做重构时成本上升

前端侧目前还是第一版原型，当前最合理的策略是：

- 先保持 `mova-web` 为一个独立应用
- 先按页面和 API 能力演进，不急着抽共享设计系统
- 等页面数量和交互复杂度明显增长后，再拆更细的前端 feature 目录或 UI 基础层

## 5. 推荐的拆分原则

### 4.1 先拆文件，再决定要不要拆 crate

当前最合理的策略不是继续拆 workspace，而是先把大文件按 feature 和职责拆开。

下一轮优先顺序建议：

1. 先拆 `mova-application/src/metadata.rs`
2. 再拆 `apps/mova-server/src/response.rs`
3. 再拆 `apps/mova-server/src/sync_runtime.rs`
4. 之后再把 `mova-server` 调整成更明显的 feature-first 目录

### 4.2 按业务能力拆，而不是按“技术动作”拆

例如：

- “媒体库管理”
- “媒体浏览 / catalog”
- “扫描与同步”
- “元数据补全”
- “播放进度”

比起按 `query.rs` / `utils.rs` / `service.rs` 这种偏抽象名字拆，更容易长期维护。

### 4.3 不要为了“看起来干净”而过度抽象

当前阶段不建议引入：

- repository trait 层层包裹 SQL
- handler/service/repository/entity 过度模板化
- 为每个小功能都建一个 crate

现在最重要的是让边界清楚，而不是让目录数量变多。

## 6. 推荐目标结构

### 6.1 `apps/mova-server`

当前 `handlers/` 和 `routes/` 是平铺的，短期还能用，但继续长下去会越来越散。

更推荐未来逐步收敛到：

```text
apps/mova-server/src/
  bootstrap/
    app.rs
    config.rs
    main.rs
  http/
    libraries/
      handlers.rs
      routes.rs
      response.rs
    catalog/
      handlers.rs
      routes.rs
      response.rs
    playback/
      handlers.rs
      routes.rs
      response.rs
  sync/
    registry.rs
    runtime.rs
  error.rs
```

重点不是一次性改完，而是以后新增接口时尽量按 feature 往里收，而不是继续在根下平铺。

### 6.2 `crates/mova-application`

更适合往 feature 目录收：

```text
crates/mova-application/src/
  libraries/
  catalog/
  metadata/
  playback/
  sync/
  error.rs
  lib.rs
```

其中：

- `sync/`
  - 全库扫描
  - 路径级增量同步
  - 扫描和显式路径同步调用的应用层入口
- `metadata/`
  - provider trait
  - TMDB client
  - 匹配规则
  - metadata merge

### 6.3 `crates/mova-db`

这里的第一步已经做完，但目标结构仍然成立。

更推荐收敛成：

```text
crates/mova-db/src/
  libraries.rs
  catalog_query.rs
  catalog_sync.rs
  series.rs
  playback_progress.rs
  scan_jobs.rs
  users.rs
  pool.rs
  lib.rs
```

建议分法：

- `catalog_query.rs`
  - 列表、详情、媒体文件、season/episode 查询
- `catalog_sync.rs`
  - 按路径增量 upsert/delete
  - 全库同步
  - `media_files` 写入更新
- `series.rs`
  - `series / seasons / episodes` 聚合写入和清理

### 6.4 `crates/mova-scan`

这里的第一步也已经做完，当前目录已经接近这个目标：

```text
crates/mova-scan/src/
  discover.rs
  parse.rs
  sidecar.rs
  probe.rs
  lib.rs
```

建议职责：

- `discover.rs`
  - 文件发现、递归遍历、支持扩展名判断
- `parse.rs`
  - 标题清洗、年份提取、季集号提取
- `sidecar.rs`
  - `.nfo`、海报、背景图发现
- `probe.rs`
  - `ffprobe` 调用与输出解析

## 7. 功能拆分与聚合建议

### 应该聚合的

- 扫描任务、路径级增量同步和手动扫库编排
  - 这些本质上都是同一个 “library sync” 能力
- TMDB provider、标题匹配、metadata merge
  - 这些本质上都是同一个 “metadata enrichment” 能力
- 顶层媒体列表、详情、season/episode 展示
  - 这些本质上都是同一个 “catalog” 能力

### 应该拆开的

- “媒体浏览” 和 “扫描入库”
  - 一个是读侧，一个是写侧
- “HTTP response mapping” 和 “应用层业务”
  - server 只做协议层转换，不要把业务规则塞进 response 组装
- “文件解析” 和 “远程元数据补全”
  - 本地扫描和远程 enrichment 需要能分别演进

## 8. 现在最值得做的结构优化

如果要开始真正动目录，我建议顺序是：

1. 先拆 `crates/mova-application/src/metadata.rs`
2. 再拆 `apps/mova-server/src/response.rs`
3. 再拆 `apps/mova-server/src/sync_runtime.rs`
4. 最后把 `apps/mova-server` 改成按 feature 组织

这个顺序的好处是：

- 风险最低
- 每一步都能单独验证
- 不会影响当前 API 形态
- 能让后续前端开发时后端结构更稳定
- 前两步都主要发生在 crate 内部，不会打断当前调用方

## 9. 结论

当前项目不是 “crate 边界混乱”，而是 “crate 内部仍有少量超大文件”。

所以近期最优策略应该是：

- 保留当前 5 个 crate
- 不急着继续拆 workspace
- 继续按 feature 和职责拆大文件
- 让 server 逐步从“平铺模块”过渡到 “feature-first”

这条路线比继续补更多 crate、或者为了整齐做过度抽象，更符合当前阶段的可维护性目标。
