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

- 当前任务的唯一代码修改边界是本项目根目录（即本文件所在的仓库）。除非用户在当前请求中明确点名并授权修改另一个项目，否则不得在项目根目录之外新增、修改或删除文件，也不得对其他仓库执行暂存、提交、推送或发布操作。
- 工作区中同时挂载、可读取或可写入其他项目，不代表获得了修改授权。跨项目联动默认只在本项目内完成服务端契约和文档，并在最终说明中给出其他项目的适配要求；不得直接修改其他项目。
- 默认不要求宿主机安装 Rust 或 Python，优先选择当前环境里最直接、最低摩擦的验证方式。
- 这台开发机已经安装 Rust，Rust 相关的 `cargo check` / `cargo test` 可以直接在宿主机运行；只有在本机能力不足或任务明确需要隔离环境时再回退到 Docker。
- 用户可见文案默认以英文为主，除非当前任务明确要求中文或其他语言。
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
- `scope` 尽量具体，优先使用类似 `player`、`scan`、`settings`、`libraries`、`auth`、`api` 这类明确范围，不要自动使用某些 skill，信息要用英文描述。
- 不确定时先读代码再改，不凭印象、不凭过时上下文直接下手。
- 涉及数据库 schema 改动时，必须明确说明：
  - 旧数据库是否可以平滑迁移
  - 还是需要重建数据库 / 清理数据目录

## 构建与发布

- 当用户在本仓库中说“构建且发布”“构建并发布”或“发布镜像”时，默认含义是：使用当前工作区代码构建测试期 Docker 镜像并推送到 Docker Hub。
- 默认发布镜像为 `richeschiu/mova:latest`。
- 默认发布命令为：
  `./scripts/publish-docker-images.sh`
- 当前测试期默认发布 Linux 多架构镜像：`linux/amd64` 和 `linux/arm64`。Windows 和 macOS 用户通过 Docker Desktop 运行这个 Linux 镜像；Linux 用户通过 Docker Engine / Docker Desktop 运行同一镜像。
- 发布脚本默认会检查构建基础镜像是否已经包含上述平台；缺失时先发布 `docker/base` 下的基础镜像，再发布主镜像。需要强制重发基础镜像时，使用 `MOVA_PUBLISH_BASE_IMAGES=1 ./scripts/publish-docker-images.sh`。
- 这条命令固定从仓库根目录执行；不要因为上下文丢失再搜索 Dockerfile 或 compose 文件位置，除非该命令实际失败且错误明确指向路径变更。
- 发布完成后，必须运行 `docker buildx imagetools inspect richeschiu/mova:latest`；默认发布脚本已经包含这一步。
- 最终说明里要写清楚镜像 digest、已发布平台，以及当前未提交改动是否已经被包含进镜像。发布平台应至少包含 `linux/amd64` 和 `linux/arm64`；如果某个平台失败，必须明确说明。
- “构建且发布”本身视为用户对推送镜像的明确授权；如果用户只说“构建”或表达不明确，只构建或先确认，不要擅自推送。
- 构建发布不等于提交代码；除非用户同时要求提交，否则不要自动 commit。

## 当前版本阶段

- 当前版本尚未进入 `1.0`，仍处于 pre-1.0 快速迭代阶段。
- 默认接受破坏性改动。功能、API、schema、UI、配置、数据结构、目录约定都可以直接按新设计调整。
- 是否启用新 schema、重构旧字段或调整数据结构，以当前产品模型和技术合理性为准，不以兼容旧设计为优先目标。
- 如果当前数据库字段、表关系或数据模型已经不适合新功能，应直接重构 schema，不要为了迁就旧结构把业务逻辑写复杂。
- 废弃字段、废弃 UI、废弃路径、旧 API、旧配置和旧数据结构不做兼容保留，能删就删。
- 如果一条旧路径已经明确被替代，直接删除旧逻辑，不要额外叠兼容层、双路径、迁就旧数据的兜底逻辑。
- 不要为了兼容历史行为写兜底性代码；实现应对齐当前产品方向和最新数据模型。
- 即使兜底看起来“更安全”，也不要主动增加用于保留旧行为、旧字段、旧配置或旧数据的 fallback/default branch。
- 如果破坏性改动会影响已有数据或部署，最终说明里直接写清楚需要重建、重新扫描或重新配置。
- 只有当用户在当前对话明确要求兼容旧版本时，才考虑迁移层、兼容路径或兜底逻辑。

## 数据库阶段规则

- 用户明确确认进入 `1.0` 稳定阶段之前，数据库保持单迁移文件：`migrations/0001_init.sql`。
- 这个阶段做 schema 变更时，直接修改 `migrations/0001_init.sql`，不要新增 `0002`、`0003` 这类后续 migration。
- 判断是否需要 schema 变更时，优先看数据模型是否清晰、字段职责是否准确、查询和业务逻辑是否自然；不要因为旧库存在就强行沿用不合理字段。
- 如果新功能依赖更合理的数据表达方式，允许直接改表、改字段、拆表或合并字段，再同步更新 Rust 查询、响应类型、前端类型和文档。
- 修改 `migrations/0001_init.sql` 不会自动更新已经执行过迁移的旧数据库，默认需要重建数据库 / 重置数据目录。
- 做 schema 变更时，默认走重建数据库 / 重置数据目录路径；不要为了旧数据库新增兼容 migration。
- 本地构建或重启前，如果当前改动包含数据库 schema、migration 或依赖旧数据结构的持久化模型变更，直接删除开发数据库数据并重新初始化，不创建备份；仅修改查询、业务逻辑或文档时不删除数据库。
- 如果需要重建 `data/postgres` 或重新初始化数据库，最终说明里必须明确写出来。

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

- 如果行为变了，要把新的预期行为写清楚。
