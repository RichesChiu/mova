# Mova Migrations AGENTS

本文件适用于 `migrations` 下的数据库迁移。公共协作规则统一看根目录 `AGENTS.md`，这里只保留 schema 变更执行细节。

## 当前仓库事实

- 项目仍处于 pre-1.0 阶段，允许激进改动。
- 目前数据库仍是单迁移文件：`migrations/0001_init.sql`。
- 修改 `migrations/0001_init.sql` 不会自动更新已经执行过迁移的旧数据库。

## Schema 改动规则

- 做 schema 变更时，必须明确当前走的是哪条路径：
  - 新增 migration，兼容旧数据库。
  - 或者在当前激进开发阶段要求重建数据库 / 重置数据目录。
- 如果需要重建 `data/postgres` 或重新初始化数据库，最终说明里必须明确写出来。
- 废弃字段在当前阶段不需要保留兼容，除非用户当轮明确要求兼容。

## Markdown 同步

- 数据库字段影响 API 请求、响应或行为时，检查并更新 `docs/API.md`。
- 数据库重建、迁移方式或初始化流程变化时，检查并更新相关 README。
