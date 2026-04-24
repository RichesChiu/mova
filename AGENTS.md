# AGENTS

这份文件保持简短，只记录当前仓库里最高优先级、最稳定的 AI 协作规则，不重复 `README.md`、`docs/API.md` 和目录级 `AGENTS.md` 里的执行细节。

如果规则冲突，按下面顺序执行：
1. 当前用户在对话里的明确要求
2. `AGENTS.md`
3. 其他项目文档

如果多个 `AGENTS.md` 同时适用，根目录 `AGENTS.md` 负责全局规则，离被改文件更近的目录级 `AGENTS.md` 负责该区域的执行细节。

写代码时，规范、约束、协作方式默认只以适用的 `AGENTS.md` 为准。
`README.md`、`docs/API.md`、`apps/*/README.md`、`crates/*/README.md` 这类文档只在当前任务确实需要产品说明、接口字段、运行方式或模块背景时按需查阅，不作为默认规范来源。

## 协作规则

- 默认不要求宿主机安装 Rust 或 Python，优先选择当前环境里最直接、最低摩擦的验证方式。
- 这台开发机已经安装 Rust，Rust 相关的 `cargo check` / `cargo test` 可以直接在宿主机运行；只有在本机能力不足或任务明确需要隔离环境时再回退到 Docker。
- 用户可见文案默认以英文为主，除非当前任务明确要求中文或其他语言。
- 当前开发阶段改动可以更激进，废弃字段、废弃 UI、废弃路径不做兼容保留，能删就删。
- 如果一条旧路径已经明确被替代，优先删除旧逻辑，不要额外叠兼容层。
- 功能、API、行为、运行方式、产品方向发生变化时，同一轮改动里同步更新相关 markdown，不要把文档更新留到后续补。
- 项目功能改造默认要检查并更新 `README.md`；如果涉及 API 变动，默认还要检查并更新 `docs/API.md` 以及受影响模块的相关 markdown。
- 做文档同步时，不只看单一文件，要主动关注相关文档是否也需要一起更新，例如：
  - 总体使用方式或能力变化：`README.md`
  - API、请求/响应、路由、字段变化：`docs/API.md`
  - 分区职责、运行方式、模块行为变化：对应 `apps/*/README.md`、`crates/*/README.md`、`docs/*`
- 提交前至少跑与改动范围对应的检查，例如 `cargo check`、前端 `tsc`、前端 `vite build`、定向测试。
- 只说明自己真的跑过的检查、测试和构建结果，不要把推测运行效果写成“已经验证通过”。
- 提交信息统一使用 conventional commits，例如：
  - `feat(scope): ...`
  - `fix(scope): ...`
  - `refactor(scope): ...`
  - `docs(scope): ...`
  - `chore(scope): ...`
- `scope` 尽量具体，优先使用类似 `player`、`scan`、`settings`、`libraries`、`auth`、`api` 这类明确范围。
- 不确定时先读代码再改，不凭印象、不凭过时上下文直接下手。
- 涉及数据库 schema 改动时，必须明确说明：
  - 旧数据库是否可以平滑迁移
  - 还是需要重建数据库 / 清理数据目录

## 项目结构速览

- `apps/mova-server`
  Rust HTTP 服务和运行时入口。
- `apps/mova-web`
  React + Vite 前端。
- `crates/mova-application`
  应用层业务逻辑。
- `crates/mova-db`
  SQL、持久化、同步逻辑。
- `crates/mova-domain`、`crates/mova-scan`
  共享模型、媒体发现与探测。
- `migrations`
  数据库迁移。
- `scripts`
  辅助脚本，包括 Python 媒体分析任务。

## 目录级 AGENTS 分工

- `apps/mova-web/AGENTS.md`
  负责 `apps/mova-web` 的 UI、交互、样式、播放器界面和前端验证。
- `apps/mova-server/AGENTS.md`
  负责 Rust HTTP 服务、路由、handler、bootstrap 和 runtime glue。
- `crates/AGENTS.md`
  负责应用层业务逻辑、数据库访问、领域模型、扫描链路和 Rust crate 验证。
- `migrations/AGENTS.md`
  负责数据库迁移、schema 变更和重建数据库说明。
- `scripts/AGENTS.md`
  负责 Python / 辅助脚本、媒体分析任务和脚本侧验证。
- 如果任务跨目录，所有相关目录的 `AGENTS.md` 都要一起遵守；不要把跨领域公共规则重复写进目录级文件。

## 给 AI 的补充说明

- 让实现对齐当前产品方向，不要主动保留过时行为。
- 如果行为变了，要把新的预期行为写清楚。
