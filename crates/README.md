# Rust Crates

`crates` 包含 Mova 服务端的核心 Rust 能力。HTTP 入口位于
`apps/mova-server`，业务通过以下依赖方向向内调用：

```text
apps/mova-server
    └── mova-application
        ├── mova-db
        └── mova-scan

mova-domain 提供跨层共享的领域模型
```

## 分层职责

| Crate | 职责 | 不应包含 |
| --- | --- | --- |
| `mova-application` | 业务用例、输入校验和跨模块编排 | HTTP 路由、原始 SQL |
| `mova-db` | PostgreSQL 连接、显式 SQLx 查询、事务和持久化映射 | HTTP、文件分析、业务流程编排 |
| `mova-domain` | 共享领域模型和不涉及 IO 的小型 helper | SQL、HTTP、文件系统或远端请求 |
| `mova-scan` | 文件发现、拆名、sidecar、`ffprobe`、音轨和字幕发现 | 数据库写入、TMDB、扫描任务状态 |

`mova-db` 当前直接使用显式 SQLx 查询，不额外引入只转发调用的
repository trait。各 crate 的模块和公开导出以 `src/lib.rs` 为准，依赖以
`Cargo.toml` 为准；README 不维护容易过时的函数或文件清单。

## 相关规范

- HTTP API：[`../docs/API.md`](../docs/API.md)
- 媒体库扫描：[`../docs/MEDIA_LIBRARY_SCAN.md`](../docs/MEDIA_LIBRARY_SCAN.md)
- SSE 同步：[`../docs/SSE.md`](../docs/SSE.md)
- 缓存生命周期：[`../docs/LIBRARY_CACHE_LIFECYCLE.md`](../docs/LIBRARY_CACHE_LIFECYCLE.md)
- TMDB 接口参考：[`../docs/TMDB.md`](../docs/TMDB.md)
- TMDB 接入策略：[`../docs/TMDB_INTEGRATION.md`](../docs/TMDB_INTEGRATION.md)

Rust 改动优先运行受影响 package 的 `cargo check -p ...` 和
`cargo test -p ...`。跨 API 或用户流程的改动还需验证相应客户端。
