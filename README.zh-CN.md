<p align="center">
  <img src="apps/mova-web/public/mova-logo-master-transparent.png" alt="Mova 标志" width="96" />
</p>

<h1 align="center">Mova</h1>

<p align="center">
  面向本地电影和剧集的自托管媒体服务器，围绕自动整理、元数据补全和顺滑播放体验构建。
</p>

<p align="center">
  <a href="README.md">英文版</a> | 中文
</p>

![Mova 主页](docs/assets/readme/home.png)

## Mova 是什么

Mova 是一个用于整理、浏览和播放本地电影与剧集的自托管媒体服务器。它的目标不是做复杂后台，而是把挂载目录、扫描媒体库、补齐元数据、继续观看和管理访问权限这些常见动作收成一条轻量、清晰的网页使用链路。

当前版本定位为可用的 MVP，适合本机、家用服务器和私人媒体库场景。

## 产品优势

- 自动识别电影和剧集：创建媒体库后会自动首扫，后续通过 `Scan Library` 同步新增、删除、改名和移动。
- 本地结构优先可用：即使没有配置 TMDB token，也会按目录和文件名兜底导入和展示媒体。
- 更像产品的网页体验：首页、媒体库、详情页和播放器都围绕日常使用设计，而不是普通管理面板。
- 元数据按需补全：演员、海报、背景图、IMDb 评分和片头数据只在需要时拉取或分析，并持久保存。
- 面向客户端扩展：浏览器端使用 session 登录，原生客户端可以使用 token 登录流程接入。

## 核心功能

- 媒体库自动首扫和手动重扫
- 电影、剧集自动聚合和本地兜底元数据
- 电影多版本文件选择
- 剧集季列表、集列表、下一集和继续观看
- 播放进度保存和接近片尾自动完成
- 字幕切换、音轨切换和资源文件技术信息展示
- `Skip Intro` 片头跳过，并在当前资源缺少片头数据时按需分析
- 深色 / 浅色主题，中英文界面偏好，并保存到当前浏览器
- `Primary Admin`、管理员和成员三级角色
- 面向成员的媒体库访问权限控制

## 截图

### 详情页和浅色主题

![Mova 详情页浅色主题](docs/assets/readme/theme.png)

### 服务器设置

![Mova 服务器设置](docs/assets/readme/server-setting.png)

## 部署

### 环境要求

- Docker
- Docker Compose
- 一个宿主机上的本地媒体目录

### 配置

```bash
cp .env.example .env
```

常用配置：

```env
MOVA_MEDIA_ROOT=/absolute/path/to/media
MOVA_TMDB_ACCESS_TOKEN=
MOVA_OMDB_API_KEY=
HTTP_PROXY=
HTTPS_PROXY=
```

- `MOVA_MEDIA_ROOT` 必填，会只读挂载到容器内固定目录 `/media`。
- `MOVA_TMDB_ACCESS_TOKEN` 可选，不填也能扫描、入库和播放。
- `MOVA_OMDB_API_KEY` 可选，配置后会在拿到 `imdb_id` 时补 IMDb 评分。

### 启动

```bash
docker compose up -d --build
```

默认地址：

- Web: `http://127.0.0.1:36080`
- Health: `http://127.0.0.1:36080/api/health`

### 首次使用

1. 容器启动后打开 Web 页面。
2. 在初始化页面创建第一个管理员。
3. 进入服务器设置并创建媒体库。
4. 选择容器内 `/media` 下的目录。
5. 保存媒体库后，Mova 会自动开始第一次扫描。

### 数据目录

运行数据主要写入：

- `data/postgres/`
- `data/cache/`

媒体目录只读挂载，Mova 不会修改你的原始媒体文件。

开发阶段如果可以接受重建本地数据，可以清理数据库目录后重启：

```bash
rm -rf data/postgres
docker compose up -d --build
```

## 文档

- API: [docs/API.md](docs/API.md)
- 前端: [apps/mova-web/README.md](apps/mova-web/README.md)
- 后端: [apps/mova-server/README.md](apps/mova-server/README.md)
- Crates: [crates/README.md](crates/README.md)

## 路线图与反馈

Mova 仍在积极迭代中。作者也在积极维护 Pad 和 macOS 客户端方向，让它们可以更自然地接入同一个自托管媒体服务器。

欢迎提交反馈、功能建议、客户端接入想法和体验改进意见。

## 许可证

当前许可证：`AGPL-3.0-only`。详见 [LICENSE](LICENSE)。
