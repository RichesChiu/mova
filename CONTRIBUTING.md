# 为 Mova 贡献代码

[English](CONTRIBUTING.en.md) · 简体中文

感谢你帮助改进 Mova。每次贡献应保持目标单一、便于审查，并确保现有部署可以安全升级。

## 开始实现之前

先搜索已有的 [Issues](https://github.com/RichesChiu/mova/issues) 和 Pull Requests。

当改动涉及产品行为、公开 API、数据库结构、部署契约、媒体扫描规则或需要架构讨论时，请先创建 Issue。需要复现调查的缺陷，以及跨多个会话或贡献者的工作，也应先创建 Issue。小型文档、测试和维护改动可以直接提交 Pull Request。

不要公开披露疑似安全漏洞，请遵循 [SECURITY.md](SECURITY.md)。

## 分支与提交

外部贡献者应从自己的 fork 开发。从最新 `master` 创建分支，一个分支只解决一个目标，并使用小写 kebab case：

```text
feat/continue-watching-filter
fix/scan-progress-regression
refactor/realtime-dispatcher
docs/docker-deployment
test/player-shortcuts
ci/pull-request-checks
chore/dependency-refresh
```

提交信息使用英文 [Conventional Commits](https://www.conventionalcommits.org/) 并包含明确 scope：

```text
feat(player): add episode navigation
fix(scan): preserve authoritative progress
docs(api): document notification events
```

标题应简洁并使用祈使语气。非显而易见的决策写进正文；破坏性变更使用 `BREAKING CHANGE:` footer。不要把无关重构或格式化混入功能和修复提交。

## 本地开发

根目录的 `docker-compose.yml` 用于运行已发布镜像。运行当前源码应使用：

```bash
cp .env.example .env
# 在 .env 中设置 MOVA_MEDIA_PATH，以及可选的 TMDB 或代理配置。
docker compose -f compose.source.yaml up -d --build
```

源码服务监听 `http://127.0.0.1:36080`，PostgreSQL 数据和可重建缓存在 `data/` 下，媒体目录只读挂载。

```bash
docker compose -f compose.source.yaml logs -f app
docker compose -f compose.source.yaml down
```

不要混用部署 Compose 和源码 Compose。不要提交凭据、本地数据库、媒体、缓存、生成产物或私有日志。

## 验证

运行与改动风险相匹配的检查，并为行为变更补充测试。

```bash
# Web
pnpm -C apps/mova-web test
pnpm -C apps/mova-web check
pnpm -C apps/mova-web build

# 官网
npm --prefix apps/mova-site run check:api-docs
npm --prefix apps/mova-site run lint
npm --prefix apps/mova-site run typecheck
npm --prefix apps/mova-site run build

# Rust 示例
cargo check -p mova-server
cargo test -p mova-scan
```

可见 UI 改动应附带前后截图或短录屏。

## 契约与迁移

- 行为变化应同步更新相关 Markdown。
- 路由、请求、响应、字段、错误或语义变化时，更新 `docs/API.md` 和对应专题文档。
- 官网 API 内容必须和 `docs/API.md` 同步，并运行 `check:api-docs`。
- `README.md` 只保留产品、部署、首次使用和主要方向。
- 不要修改已经执行的迁移；新增下一个顺序迁移，并支持已初始化数据库原地升级。
- 表结构改动必须同步 Rust 查询、响应模型、TypeScript 类型、测试和文档，并说明是否需要重新扫库或重建缓存。
- HTTP 契约版本 `1` 允许新增接口、可选字段和错误码。删除或改变现有语义需要明确的版本决策；SSE 破坏性变化需要提升 `protocol_version`。

## Pull Request

PR 标题使用 Conventional Commit，因为它会成为 squash 提交信息。可合并的 PR 应：

- 有对应 Issue 时使用 `Closes #123` 关联；
- 说明结果、范围和重要取舍；
- 列出实际通过的检查；
- UI 改动附带视觉证据；
- 说明 API、数据库、部署和文档影响；
- 不包含无关或临时文件。

单一目标的 PR 通常 squash 合并到 `master`。仓库内的源分支会在合并后自动删除；来自 fork
的贡献者需要自行删除 fork 中的对应分支。

## 发布

普通功能、修复、文档和维护 PR 不会触发发布。维护者从最新 `master` 创建
`chore/release-X.Y.Z` 分支，更新 `Cargo.toml` 和 `Cargo.lock` 中的工作区版本，并使用以下
固定 PR 标题：

```text
chore(release): prepare X.Y.Z
```

该 PR 合并且对应的 `master` CI 全部通过后，`Release` 工作流会自动：

1. 从通过验证的提交创建带注释的 `vX.Y.Z` 标签；
2. 构建、冒烟验证并安全扫描 Linux `amd64` 和 `arm64` 镜像；
3. 发布不可变镜像 `richeschiu/mova:X.Y.Z`；
4. 稳定版更新 `latest`，SemVer 预发布版更新 `preview`；
5. 生成并发布对应的 GitHub Release。

验证通过并晋升后的 `publish-*` 候选标签会立即删除。构建或验证失败的候选最多保留 72
小时用于诊断，随后由每日清理任务回收；清理只按标签名删除候选，不会删除共享镜像内容、
不可变版本标签、`latest` 或 `preview`。

版本标签已指向其他提交时，工作流会停止发布。同一已打标签提交的失败任务可以在
GitHub Actions 中安全重试并续传未完成的发布；不要手工重建或移动发布标签。仓库维护者
需要配置 Actions variable `DOCKERHUB_USERNAME` 和具有读取、写入、删除权限的 Actions
secret `DOCKERHUB_TOKEN`。
