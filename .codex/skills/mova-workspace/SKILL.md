---
name: mova-workspace
description: 在 Mova 仓库中处理通用仓库工作流、Rust 后端、数据库、扫描链路、Docker 运行方式和文档同步。前端 UI/交互细节请配合 mova-frontend skill 使用。
---

# Mova Workspace

这个 skill 用于当前 Mova 仓库的通用工作流。  
`AGENTS.md` 负责最高优先级协作规则；这个 skill 主要负责执行路径、代码地图、验证方式和仓库事实。

## 最小阅读顺序

- 先看 `AGENTS.md`
- 其余项目文档没有固定高低顺序，按当前任务相关性选择：
  - `README.md`
  - `docs/API.md`
  - `docs/ROADMAP.md`
- 需要时再看分区文档：
  - 前端：`apps/mova-web/README.md`
  - 后端：`apps/mova-server/README.md`
  - crates：`crates/README.md`

## 当前仓库事实

- 项目仍处于 pre-1.0 阶段
- 目前数据库仍是单迁移文件：`migrations/0001_init.sql`
- Library watcher 已经移除
- 新建且启用的媒体库会自动触发一次扫描
- 新增、重命名、移动、删除文件统一通过手动 `Scan Library` 做 reconcile

## 数据库改动规则

- 修改 `migrations/0001_init.sql` 不会自动更新已经执行过迁移的旧数据库
- 做 schema 变更时，必须明确当前走的是哪条路径：
  - 新增 migration，兼容旧数据库
  - 或者在当前激进开发阶段要求重建数据库 / 重置数据目录
- 如果需要重建 `data/postgres` 或重新初始化数据库，最终说明里必须明确写出来

## 后端职责边界

- `apps/mova-server` 只放 HTTP、bootstrap、runtime glue
- 业务逻辑放 `crates/mova-application`
- SQL 和持久化放 `crates/mova-db`
- 共享领域模型放 `crates/mova-domain`
- 扫描、解析、探测、sidecar 逻辑放 `crates/mova-scan`
- API 路由统一保持在 `/api`

## 验证方式

- Rust 侧优先走 Docker-first 的定向验证，例如 `cargo check -p ...`
- 前后端一起改时，要验证两边

## Markdown 同步

- 具体文档同步规则以 `AGENTS.md` 为准
- 这个 skill 只补充执行层面的定位：
  - 前端结构/职责变化：更新 `apps/mova-web/README.md`
  - 后端启动/路由/runtime 变化：更新 `apps/mova-server/README.md`
  - crate 职责变化：更新对应的 `crates/*/README.md`

## 边界

- 这个 skill 负责仓库结构认知，以及后端 / 数据库 / 扫描链路的改动入口
- 具体前端 UI / 交互 / 视觉细则交给 `mova-frontend` skill
- 不把自己写成第二份 roadmap，也不重复 `AGENTS.md` 里的协作规则
