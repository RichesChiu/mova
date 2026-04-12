# Mova

Mova 是一个自托管媒体服务器项目，目标很直接：

- 把本地媒体目录整理成可持续同步的媒体库
- 用更轻松的方式浏览、播放和续播
- 让扫描、补元数据、同步这些内部过程尽量自动完成

当前仓库包含两个应用和四个核心 Rust crate：

- `apps/mova-server`
- `apps/mova-web`
- `crates/mova-application`
- `crates/mova-db`
- `crates/mova-domain`
- `crates/mova-scan`

## 产品特点

### 好看且直接的 Web 体验

Mova 的前端不是只做“能用”的后台页，而是尽量把首页、媒体库页、详情页和播放器做得更像一个真正会长期打开的媒体应用：

- 首页和媒体库页会直接展示扫描中的进度和占位卡
- 首页、媒体库页和详情页都会尽量把扫描中的进度和占位过程前台化
- 扫描中的占位优先按“电影文件 / 剧集目录组”展示，避免剧集在元数据拉取前先被拆成一排单集卡片
- 扫描失败也会在首页库卡和媒体库页直接提示出来，而不是藏在后台任务细节里
- 扫描中的占位卡会尽量保持和最终卡片接近的尺寸，减少同步完成前后的 UI 抖动
- 如果整库写入因为单条脏数据失败，扫描会自动回退到逐条写入，尽量保住其余正常条目继续入库，而不是整轮中断
- 对于未改路径、且过去已经成功补过 metadata / 海报的条目，重扫会优先复用数据库里已落下的结果，不再每次都重复请求 TMDB / OMDb 和重新下载图片
- 扫描不会阻塞浏览和进入详情
- 即使 TMDB 或 OMDb 暂时不可用，扫描也会优先按本地目录和文件名规则聚合剧集；像 `Season 1/01.mkv`、`EP02`、`Episode 03`、`第03集` 这类常见命名不会因为远端超时就被拆成一堆单片
- TMDB 匹配时，本地解析出的年份只作为软提示；如果年份写错或带偏了搜索，系统会自动回退到不带年份的结果
- 当文件名本身夹杂发布组、编码参数或 HTML 实体时，扫描也会优先参考更干净的父目录名称来推断电影标题，避免像 `A Writer&#39;s Odyssey...` 这类文件名把标题识别偏
- 同一部电影如果本地存在多个不同封装或不同版本文件，列表里会收成同一个条目，并在详情页先通过版本选择器选好具体资源文件再进入播放
- 影片详情页会先展示当前资源文件的 source details，再并排展示视频、音频、字幕三类技术卡片；电影版本本身在播放区切换，音轨和字幕则各自通过卡头的小下拉切换。卡片会展示路径、体积、分辨率、码率、色彩信息、音轨参数，以及字幕的默认、强制、听障和外挂标记，方便比较多版本文件
- 影片详情页标题旁边会直接展示带 `IMDb` 标识的评分；如果配置了可选的 `MOVA_OMDB_API_KEY`，扫描和元数据刷新时会补齐这条评分
- 详情页 hero 会优先使用背景图或海报做模糊大底，并叠加更重的渐变和光晕；顶部不再重复堆叠副标题，而是把年份、国家/地区、题材类型和工作室等信息收进更稳定的 facts 区，让海报、标题和 IMDb 评分更像一个完整的观影页
- 详情页主体现在会先渲染，再异步拉演员列表，避免首次进入电影或剧集详情时被远端 cast 刷新拖慢首屏
- 库页和详情页里的返回入口会统一成轻量文本链接，而不是厚重的按钮块，减少页面工具条的臃肿感
- 播放器支持续播、从头播放、字幕切换、音轨切换、缓冲反馈、重试、临近片尾自动判定看完，以及自动播放或全屏失败时的非阻断提示

### 更适合长时间运行的 Rust 后端

后端使用 Rust 实现，重点放在这几类长期运行场景：

- 媒体库扫描
- 文件 watcher 和后台路径校准
- 流媒体直链输出
- 播放进度与继续观看状态维护

对这个项目来说，Rust 的价值不只是“快”，更是更稳、更适合做持续运行的同步和服务端逻辑。

当前同步策略也已经往更轻的方向收了：

- 优先依赖文件 watcher 感知新增、删除和常见改动
- 服务启动后只做一次延迟校准，避免刚启动就立刻全库扫盘
- 后续的后台校准只在库被标记为 dirty 时才做低频兜底，而不是固定几分钟全库扫一次

### 尽量减少用户操作成本

Mova 现在的方向不是让管理员维护很多后台细节，而是把常见动作尽量收敛成简单流程：

- 启动后先登录或初始化首个管理员
- 在设置页里创建媒体库、编辑库、管理用户
- 设置页和个人账号这类轻量操作尽量收进弹窗，而不是铺满整页表单
- 选一个目录作为库根路径，然后让系统自己扫描和同步

扫描进度、库状态和条目占位会尽量前台化，而不是要求用户盯着任务面板。

## 快速启动

先复制环境变量模板：

```bash
cp .env.example .env
```

最常用的配置通常只需要这几个：

```env
MOVA_MEDIA_ROOT=/mnt/media
MOVA_TMDB_ACCESS_TOKEN=
MOVA_OMDB_API_KEY=
HTTP_PROXY=
HTTPS_PROXY=
```

然后启动：

```bash
docker compose up -d --build
```

启动后的默认访问地址：

- 本机：`http://127.0.0.1:36080`
- 远程服务器：`http://<服务器IP>:36080`
- 健康检查：`GET http://<服务器IP>:36080/api/health`

补充说明：

- `MOVA_MEDIA_ROOT` 是宿主机路径，会以只读方式挂到容器内固定目录 `/media`
- 前端建库时会直接展示容器内 `/media` 的递归目录树，所以通常不需要手写库路径
- `MOVA_TMDB_ACCESS_TOKEN` 不配置时，TMDB 自动补全会关闭，但本地扫描、入库和播放仍然可用
- `MOVA_OMDB_API_KEY` 是可选项；配置后会在 TMDB 已拿到 `imdb_id` 的前提下补齐 IMDb 评分，不配置也不影响扫描和播放
- `NO_PROXY` 会在 compose 里自动补默认值；大多数用户不需要自己配置

如果你的服务器地址是 `192.168.50.3`，启动后直接访问：

```text
http://192.168.50.3:36080
```

## 当前范围

根 README 不再展开列所有能力细节，当前仓库的功能状态拆到各子文档里维护：

- 产品和启动入口看这里
- API 和 SSE 契约看 `docs/API.md`
- 功能现状、MVP 缺口和后续开发顺序看 `docs/ROADMAP.md`
- MVP 阶段的部署和升级步骤看 `docs/DEPLOYMENT.md`
- 前端实现细节看 `apps/mova-web/README.md`
- 后端实现细节看 `apps/mova-server/README.md`
- 各 crate 的职责和入口看 `crates/*/README.md`

## 文档入口

### 顶层文档

- 接口说明：[docs/API.md](docs/API.md)
- 功能现状与开发路线：[docs/ROADMAP.md](docs/ROADMAP.md)
- MVP 部署与升级说明：[docs/DEPLOYMENT.md](docs/DEPLOYMENT.md)
- 工程结构与重构建议：[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)

### 应用文档

- 前端原型说明：[apps/mova-web/README.md](apps/mova-web/README.md)
- 后端服务说明：[apps/mova-server/README.md](apps/mova-server/README.md)

### Workspace crate 文档

- Workspace crate 索引：[crates/README.md](crates/README.md)
- 应用层 crate：[crates/mova-application/README.md](crates/mova-application/README.md)
- 持久层 crate：[crates/mova-db/README.md](crates/mova-db/README.md)
- 领域模型 crate：[crates/mova-domain/README.md](crates/mova-domain/README.md)
- 扫描能力 crate：[crates/mova-scan/README.md](crates/mova-scan/README.md)

## 开源与许可证

当前仓库实际生效的许可证仍然是 [`LICENSE`](LICENSE) 里的 `MIT`。

项目方向上，我理解你希望 Mova 未来像 Immich 一样，保持“始终免费、始终开源”的路线。如果后续要参考 Immich 当前的 `GNU AGPL v3.0` 方向来调整 Mova 的许可证，我建议单独做一次明确的 license 变更提交，同时同步更新：

- `LICENSE`
- 根 README
- 贡献说明
- 可能受影响的发布与分发文档

这样可以避免出现“README 写的是一种方向，但仓库真正生效的是另一份许可证”的歧义。

## License

Current license: `MIT`. See [`LICENSE`](LICENSE).
