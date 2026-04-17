# Mova

Mova 是一个自托管媒体服务器，面向本地媒体库的整理、浏览和播放。  
它的目标不是做一个复杂的后台系统，而是把“导入媒体、扫描整理、展示元数据、继续播放”这些常见动作收成一条尽量轻的使用链路。

当前版本已经是可用的 MVP：

- 支持初始化首个管理员并进入完整 Web 界面
- 支持创建媒体库并自动触发首次扫描
- 支持电影和剧集的自动识别、聚合和展示
- 支持继续观看、播放进度、剧集下一集和片头跳过
- 支持管理员管理媒体库、用户和普通成员访问权限

## 产品描述

Mova 主要围绕三件事设计：

1. 本地媒体自动整理  
把宿主机目录挂进容器后，Mova 会按文件结构和命名规则识别电影与剧集，并尽量自动补齐元数据、海报和背景图。

2. 更像产品而不是后台的浏览体验  
首页、媒体库页、详情页和播放器都尽量保持“可长期使用”的媒体应用体验，而不是只做功能面板。

3. 尽量减少人工维护  
新建媒体库后会自动首扫，后续通过手动 `Scan Library` 收敛新增、删除、改名和移动；即使 TMDB 不可用，也会优先按本地目录结构兜底展示。

## 如何使用与部署

### 1. 准备环境

需要：

- Docker
- Docker Compose
- 一个宿主机上的媒体目录

先复制环境变量模板：

```bash
cp .env.example .env
```

最常用的配置通常只需要：

```env
MOVA_MEDIA_ROOT=/absolute/path/to/media
MOVA_TMDB_ACCESS_TOKEN=
MOVA_OMDB_API_KEY=
HTTP_PROXY=
HTTPS_PROXY=
```

说明：

- `MOVA_MEDIA_ROOT` 是必填项，会只读挂载到容器内固定目录 `/media`
- `MOVA_TMDB_ACCESS_TOKEN` 可选；不填时仍可扫描、入库和播放，只是不会自动补 TMDB 元数据
- `MOVA_OMDB_API_KEY` 可选；配置后会在已拿到 `imdb_id` 时补齐 IMDb 评分

### 2. 启动服务

```bash
docker compose up -d --build
```

默认访问地址：

- Web：`http://127.0.0.1:36080`
- 健康检查：`http://127.0.0.1:36080/api/health`

### 3. 首次使用

启动后：

1. 如果系统里还没有管理员，会先进入 bootstrap 页面
2. 创建首个管理员后会自动登录
3. 进入设置页创建媒体库
4. 选择容器内 `/media` 目录树中的一个根路径
5. 保存后会自动开始第一次扫描

### 4. 后续日常使用

- 在首页和媒体库页查看扫描状态与媒体条目
- 在详情页浏览电影或剧集信息
- 进入播放器继续观看、从头播放、切换字幕和音轨
- 需要同步新增或改动文件时，手动点击 `Scan Library`

### 5. 升级

如果你改了代码、前端构建产物、Rust 依赖或 Dockerfile：

```bash
docker compose up -d --build
```

如果只是调整端口、volume 或环境变量：

```bash
docker compose up -d
```

### 6. 数据目录

当前运行时主要会写：

- `data/postgres/`
- `data/cache/`

媒体目录本身只读挂载，不会被 Mova 修改。

如果当前开发阶段明确接受重建本地数据，可以直接清理数据库目录后重启：

```bash
rm -rf data/postgres
docker compose up -d --build
```

## 核心功能说明

### 媒体库与扫描

- 创建媒体库后会自动首扫
- 后续通过手动 `Scan Library` 做显式同步
- 支持电影和剧集自动识别
- 同一部电影的多个版本会聚合在同一条目下
- TMDB 不可用时仍会优先按本地目录和文件名规则兜底展示
- 对已成功补全过 metadata 的未改路径条目，重扫会优先复用已有结果

### 浏览与详情

- 首页、媒体库页和详情页会前台化展示扫描进度和占位状态
- 电影详情页支持多版本文件选择
- 详情页会展示资源文件、视频、音频、字幕等技术信息
- 支持 IMDb 评分补齐
- 演员、海报和背景图会优先使用本地缓存，减少详情页阻塞

### 播放体验

- 支持继续观看和从头播放
- 支持播放进度保存
- 支持字幕切换和音轨切换
- 剧集支持 `Next Episode`
- 支持片头跳过 `Skip Intro`
- 接近片尾会自动判定为已看完

### 用户与权限

- 首个初始化管理员会被标记为 `Primary Admin`
- `Primary Admin` 可以管理普通管理员和普通成员
- 普通管理员可以管理媒体库和普通成员，但不能管理平级管理员或主管理员
- 成员只可见自己被授权的媒体库

## 常见排查

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

## 其他文档

- API 契约：[docs/API.md](docs/API.md)
- 前端说明：[apps/mova-web/README.md](apps/mova-web/README.md)
- 后端说明：[apps/mova-server/README.md](apps/mova-server/README.md)
- Workspace crate 索引：[crates/README.md](crates/README.md)

## License

Current license: `MIT`. See [`LICENSE`](LICENSE).
