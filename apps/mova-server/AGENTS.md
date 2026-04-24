# Mova Server AGENTS

本文件适用于 `apps/mova-server` 下的 Rust HTTP 服务。公共协作规则统一看根目录 `AGENTS.md`，这里只保留服务入口和路由层执行细节。

## 职责范围

- HTTP 服务、路由、handler、bootstrap、runtime glue。
- API 路由统一保持在 `/api`。
- 业务逻辑不要堆在 handler 里，优先放到 `crates/mova-application`。
- SQL 和持久化不要放在服务入口里，放到 `crates/mova-db`。

## 当前仓库事实

- 项目仍处于 pre-1.0 阶段。
- Library watcher 已经移除。
- 新建且启用的媒体库会自动触发一次扫描。
- 新增、重命名、移动、删除文件统一通过手动 `Scan Library` 做 reconcile。

## 验证

- 服务层改动优先跑定向 Rust 验证，例如 `cargo check -p mova-server`。
- 如果改动影响应用层、数据库或扫描链路，同时遵守对应目录的 `AGENTS.md`。

## Markdown 同步

- 后端启动、路由、runtime、部署方式变化时，检查并更新 `apps/mova-server/README.md`。
- API 路由、请求、响应或字段变化时，检查并更新 `docs/API.md`。
