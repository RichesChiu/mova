<p align="center">
  <img src="apps/mova-web/public/mova-logo-master-transparent.png" alt="Mova 标志" width="96" />
</p>

<h1 align="center">Mova</h1>

<p align="center">
  面向本地电影和剧集的轻量、安全、高效自托管媒体服务器。
</p>

<p align="center">
  <a href="README.md">英文版</a> | 中文
</p>

![Mova 主页](docs/assets/readme/home.png)

## Mova 是什么

Mova 是一个用于整理、浏览和播放本地电影与剧集的自托管媒体服务器。服务端使用 Rust 构建，这是一门强调内存安全、稳定性能和资源效率的现代系统语言。

项目希望把媒体服务器体验保持得足够简单可靠：挂载媒体目录，扫描媒体库，按需补齐元数据，然后在清晰的网页界面里浏览和播放。当前版本定位为可用的 MVP，适合本机、家用服务器和私人媒体库场景。

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

启动后，Mova 会生成两个运行时目录：

- `data/postgres/`：PostgreSQL 数据库文件，用于保存媒体库、用户、元数据和播放进度。
- `data/cache/`：缓存海报、背景图和生成的媒体资源。

媒体目录只读挂载，Mova 不会修改你的原始媒体文件。

### 首次使用

1. 容器启动后打开 Web 页面。
2. 在初始化页面创建第一个管理员。
3. 进入服务器设置并创建媒体库。
4. 选择容器内 `/media` 下的目录。
5. 保存媒体库后，Mova 会自动开始第一次扫描。

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
