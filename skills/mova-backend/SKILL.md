---
name: mova-backend
description: 在 Mova 仓库中处理 Rust 后端、数据库、扫描链路、迁移和脚本。主要覆盖 apps/mova-server、crates/*、migrations、scripts；前端 UI/交互细节请配合 mova-frontend skill 使用。
---

# Mova Backend

这个 skill 专门处理当前仓库里的后端与通用执行路径。  
公共协作规则统一看 `AGENTS.md`；这个 skill 只保留 Rust / 数据库 / 扫描链路 / 文档落点等后端侧执行知识。

## 使用时机

- 修改 `apps/mova-server`
- 修改 `crates/*`
- 修改 `migrations`
- 修改 `scripts`
- 处理 Rust 侧验证、数据库说明、扫描链路和后端文档同步

## 覆盖目录

- `apps/mova-server`
  HTTP 服务、路由、handler、运行时 glue。
- `crates/mova-application`
  应用层业务逻辑。
- `crates/mova-db`
  SQL、持久化、同步逻辑。
- `crates/mova-domain`
  共享领域模型。
- `crates/mova-scan`
  媒体发现、探测、sidecar 相关逻辑。
- `migrations`
  数据库迁移文件。
- `scripts`
  Python 和其他辅助脚本。

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

- Rust 侧优先走定向验证，例如 `cargo check -p ...`、`cargo test -p ...`
- 前后端一起改时，要验证两边

## Markdown 同步

- 具体文档同步规则以 `AGENTS.md` 为准
- 这个 skill 只补充执行层面的定位：
  - 后端启动/路由/runtime 变化：更新 `apps/mova-server/README.md`
  - crate 职责变化：更新对应的 `crates/*/README.md`
  - 前端结构/职责变化：交给 `mova-frontend` skill 处理

## 边界

- 这个 skill 负责后端、数据库、扫描链路和仓库执行路径
- 具体前端 UI / 交互 / 视觉细则交给 `mova-frontend` skill
- 不把自己写成第二份 `AGENTS.md`，也不重复产品说明文档
