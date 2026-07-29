# mova-server

`mova-server` 是 MOVA 的 Rust HTTP 服务，基于 Axum 和 Tokio。它负责接入 HTTP/SSE
请求、执行鉴权与协议转换、托管 Web 构建产物，并把业务用例交给 workspace crates。

接口字段、错误码和权限语义以 [`../../docs/API.md`](../../docs/API.md) 为准。本文件只说明
服务入口、代码边界和本地验证方式，不复制具体接口或扫描算法。

## 代码边界

```text
routes -> handlers -> mova-application -> mova-db
                     \-> mova-domain
```

- `src/main.rs`：加载配置、连接数据库、启动后台 worker 和 HTTP 服务。
- `src/app.rs`：组装 `/api` 路由与 Web 静态资源 fallback。
- `src/routes/`：声明路径、HTTP 方法和 handler 绑定。
- `src/handlers/`：处理请求提取、鉴权、业务调用和 response DTO。
- `src/auth.rs`：Web session、Bearer token 与资源访问控制。
- `src/response.rs`、`src/error.rs`：统一成功响应和错误 envelope。
- `src/state.rs`：进程共享依赖与运行时句柄。
- `src/sync_runtime.rs`：持久化后台任务的领取、租约、重试与执行。
- `src/realtime.rs`：revision 通知、SSE dispatcher 和连接可见性过滤。

handler 不承载可复用业务规则，也不直接维护 SQL。业务编排放在
`crates/mova-application`，持久化放在 `crates/mova-db`，共享模型放在
`crates/mova-domain`。

## 运行时概览

服务启动后会：

1. 读取运行时配置并初始化日志。
2. 连接 PostgreSQL，执行当前 schema initialization。
3. 准备缓存目录和可选的 TMDB metadata provider。
4. 启动持久化后台任务 worker、revision listener 与 realtime dispatcher。
5. 构建 `AppState` 并开始提供 HTTP、SSE 和静态 Web 资源。

媒体扫描由数据库任务驱动，HTTP handler 只负责入队。媒体文件与字幕流属于协议和本地
文件边界，可以直接由服务层处理。删除媒体库时，权威数据由数据库级联删除，持久化缓存
清理任务负责回收库命名空间。

## 协议约定

- JSON 错误提供稳定的 `error_code` 和 `params`，客户端据此本地化；
  `message` 仅作为诊断与未知错误码兜底。
- SSE 只传资源失效通知和临时进度，不作为最终业务数据来源。
- 客户端通过资源 revision 恢复断线期间可能遗漏的变化。
- 扫描和缓存清理通知使用原因码，底层 provider、网络或 `ffprobe` 文本仅保留为诊断信息。

相关专题：

- [HTTP API](../../docs/API.md)
- [SSE 与资源同步](../../docs/SSE.md)
- [媒体库扫描](../../docs/MEDIA_LIBRARY_SCAN.md)
- [媒体库缓存生命周期](../../docs/LIBRARY_CACHE_LIFECYCLE.md)
- [TMDB API 参考](../../docs/TMDB.md)
- [TMDB 集成策略](../../docs/TMDB_INTEGRATION.md)

## 本地验证

从仓库根目录运行：

```bash
cargo check -p mova-server
cargo test -p mova-server
```

需要数据库的 ignored 集成测试必须显式提供隔离的 `DATABASE_URL`。项目部署、Docker
Compose 和镜像发布方式统一见根目录 [`../../README.md`](../../README.md) 与
[`../../CONTRIBUTING.md`](../../CONTRIBUTING.md)。
