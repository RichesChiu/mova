# mova-web

`mova-web` 是 Mova 的前端原型，基于 Vite、React、TypeScript、React Router、TanStack Query 和 Biome。

当前前端已经接入 SSE 实时事件流：扫描中的媒体库会立即显示库级进度条，发现新文件时首页和媒体库页会立刻插入临时卡片，再随着元数据和海报简介的获取过程逐步更新，最后由真实列表接管。

## 运行

```bash
pnpm install
pnpm dev
```

默认开发地址是 `http://127.0.0.1:35173`。

开发模式下，Vite 会把这些接口代理到后端 `http://127.0.0.1:36080`：

- `/api/health`
- `/api/libraries`
- `/api/media-items`
- `/api/media-files`
- `/api/playback-progress`
- `/api/seasons`

如果后端不是默认地址，可以设置环境变量：

```bash
MOVA_API_PROXY_TARGET=http://127.0.0.1:36080 pnpm dev
```

## Docker

根目录执行：

```bash
docker compose up -d --build
```

本机访问地址是 `http://127.0.0.1:36080`；如果部署在远程服务器上，则访问 `http://<服务器IP>:36080`，例如 `http://192.168.50.3:36080`。构建后的前端静态文件会被打包进 `mova-server` 镜像，由后端直接托管；API 继续走同域 `/api/*`。

## 质量工具

```bash
pnpm test
pnpm format
pnpm lint
pnpm check
```
