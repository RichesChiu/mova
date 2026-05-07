# Mova Migrations AGENTS

本文件适用于 `migrations` 下的数据库迁移。公共协作规则统一看根目录 `AGENTS.md`，这里只保留 schema 变更执行细节。

## 当前仓库事实

- 当前数据库阶段规则统一看根目录 `AGENTS.md`。
- 迁移文件入口是 `migrations/0001_init.sql`。

## Schema 改动规则

- 做 schema 变更前先确认是否只需要改 `migrations/0001_init.sql`。
- 字段影响 Rust 查询、response、TypeScript 类型或文档时，同步更新对应代码和文档。
- 最终说明必须按根目录规则写清楚是否需要重建数据库 / 重置数据目录。

## Markdown 同步

- 数据库字段影响 API 请求、响应或行为时，检查并更新 `docs/API.md`。
- 数据库重建、迁移方式或初始化流程变化时，检查并更新相关 README。
