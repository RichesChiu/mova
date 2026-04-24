# Mova Crates AGENTS

本文件适用于 `crates` 下的 Rust crate。公共协作规则统一看根目录 `AGENTS.md`，这里只保留应用层、数据库、领域模型和扫描链路执行细节。

## 职责边界

- `crates/mova-application`
  应用层业务逻辑。
- `crates/mova-db`
  SQL、持久化、同步逻辑。
- `crates/mova-domain`
  共享领域模型。
- `crates/mova-scan`
  媒体发现、解析、探测、sidecar 相关逻辑。

## 当前仓库事实

- 项目仍处于 pre-1.0 阶段。
- Library watcher 已经移除。
- 新建且启用的媒体库会自动触发一次扫描。
- 新增、重命名、移动、删除文件统一通过手动 `Scan Library` 做 reconcile。

## 数据库与扫描

- SQL 和持久化放 `crates/mova-db`。
- 扫描、解析、探测、sidecar 逻辑放 `crates/mova-scan`。
- 如果 crate 改动需要 schema 变化，同时遵守 `migrations/AGENTS.md`。

## 验证

- Rust 侧优先走定向验证，例如 `cargo check -p ...`、`cargo test -p ...`。
- 前后端一起改时，要同时验证前端和后端。

## Markdown 同步

- crate 职责、运行方式或模块行为变化时，检查并更新 `crates/README.md` 或对应 crate 的 README。
- API 行为变化时，检查并更新 `docs/API.md`。
