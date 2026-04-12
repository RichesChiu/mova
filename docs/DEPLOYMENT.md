# Mova MVP 部署与升级说明

这份文档只描述当前 MVP 阶段最实用的部署方式：单机、`docker compose`、本地媒体目录挂载。  
如果你要看接口细节，见 [`./API.md`](./API.md)；如果你要看功能现状和后续缺口，见 [`./ROADMAP.md`](./ROADMAP.md)。

## 1. 前提

- 已安装 Docker 和 Docker Compose
- 宿主机上已经有一个本地媒体目录
- 默认端口 `36080`、`5432` 没有被占用

当前推荐的目录结构大致是：

```text
mova/
  .env
  docker-compose.yml
  media/
  data/
```

## 2. 最小配置

先复制环境变量模板：

```bash
cp .env.example .env
```

MVP 阶段最常用的配置通常只需要：

```env
MOVA_MEDIA_ROOT=/absolute/path/to/media
MOVA_TMDB_ACCESS_TOKEN=
HTTP_PROXY=
HTTPS_PROXY=
```

说明：

- `MOVA_MEDIA_ROOT` 是宿主机目录，会挂到容器内固定路径 `/media`
- `MOVA_TMDB_ACCESS_TOKEN` 可选；不填时仍然可以扫描、入库和播放，只是不会自动补 TMDB 元数据
- `NO_PROXY` 不需要手动配置，`docker-compose.yml` 已经内置默认值

## 3. 启动

当前开发阶段推荐直接构建并启动：

```bash
docker compose up -d --build
```

启动后默认访问地址：

- Web：`http://127.0.0.1:36080`
- 健康检查：`http://127.0.0.1:36080/api/health`

首次打开时：

1. 如果系统里还没有管理员，会先进入 bootstrap 页面
2. 创建首个管理员后会自动登录
3. 进入设置页创建媒体库

## 4. 建库

当前建库流程是：

1. 进入设置页
2. 选择容器内 `/media` 目录树中的一个根路径
3. 选择库类型
4. 选择元数据语言
5. 决定是否启用这个媒体库

建库成功后：

- 媒体库会立即出现在设置页和首页
- 需要时再手动点击 `Scan Library`
- 首页和媒体库页会在扫描进行时显示库状态和条目级占位卡

## 5. 升级

当前项目还处于开发阶段，升级方式按“是否改了镜像内容”来区分：

### 代码、前端构建产物、Rust 依赖、Dockerfile 变了

```bash
docker compose up -d --build
```

### 只是运行参数、端口、volume、环境变量变了

```bash
docker compose up -d
```

MVP 阶段还没有切到预构建镜像发布，所以不建议把升级流程理解成 `pull` 即可。

## 6. 数据与目录

当前运行时主要会写两类目录：

- `data/postgres/`
- `data/cache/`

媒体目录本身只读挂载，不会被 Mova 修改。

如果你改了数据库 schema，并且仍处于 pre-1.0 开发阶段，当前项目默认允许直接清理本地 Postgres 数据目录后重新启动：

```bash
rm -rf data/postgres
docker compose up -d --build
```

只有在你明确接受重建本地数据的情况下才这样做。

## 7. 常见排查

### 打不开页面

- 先看 `docker compose ps`
- 再看 `docker compose logs mova-server`
- 再访问 `GET /api/health`

### 扫描没有开始

- 确认库是启用状态
- 确认宿主机 `MOVA_MEDIA_ROOT` 存在
- 确认选择的库路径是容器内 `/media` 下的目录

### 扫描有进度但没元数据

- 检查 `MOVA_TMDB_ACCESS_TOKEN` 是否有效
- 不配置 TMDB 时，本地扫描和播放仍然是正常行为

### 可以进详情但视频播不了

- 当前是浏览器直链播放，不带转码
- 如果浏览器不支持当前容器或编码，播放器会给出错误提示，但不会自动转码兜底

### SSE 没有实时刷新

- 先确认浏览器里存在常驻的 `GET /api/events`
- 当前浏览器 `EventSource` 会自动重连
- 如果重连期间漏了事件，前端会在恢复连接后补拉关键 query

## 8. 当前限制

- 默认部署说明仍以开发期 `docker compose up -d --build` 为主
- 还没有预构建镜像发布链路
- 还没有会话管理、转码、自适应码率和音轨切换
- 数据库集成测试目前需要额外提供 `DATABASE_URL`
