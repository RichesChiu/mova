# mova-web

`mova-web` 是 MOVA 的 React Web 客户端，基于 Vite、TypeScript、React Router、
TanStack Query 和 SCSS。HTTP 与 SSE 契约以 [`../../docs/API.md`](../../docs/API.md)
和 [`../../docs/SSE.md`](../../docs/SSE.md) 为准。

## 目录与入口

- `src/main.tsx`：初始化主题、语言和 React 根节点。
- `src/App.tsx`：创建 Query Client、Router 和路由树。
- `src/api/`：统一 HTTP client、DTO、媒体 URL 和开发 mock。
- `src/components/`：跨页面复用的卡片、表单、浮层和播放器组件。
- `src/pages/`：路由页面及其页面级组合逻辑。
- `src/lib/`：可测试的权限、格式化、路由、播放与状态决策。
- `src/i18n/`：中英文目录、provider 和非 React 翻译入口。
- `src/styles/`：全局 foundations、主题 tokens 与共享样式。

页面负责查询编排和布局，可复用的业务判断应下沉到 `src/lib/`。DTO 统一定义在
`src/api/types.ts`，不要在页面内重复声明接口结构。

## 路由

| 路由 | 职责 |
| --- | --- |
| `/login` | 登录与首个系统管理员初始化 |
| `/` | 首页有界快照 |
| `/libraries` | 当前用户可访问的全部媒体库 |
| `/libraries/:libraryId` | 媒体库目录与扫描运行态 |
| `/media-items/:mediaItemId` | 媒体详情、季集、演员与资源信息 |
| `/media-items/:mediaItemId/play` | 不挂载 dashboard shell 的沉浸式播放器 |
| `/continue` | 当前用户可继续观看的项目 |
| `/search` | 当前权限范围内的全局搜索 |
| `/profile` | 当前用户资料、密码和界面偏好 |
| `/settings` | 有权限用户的服务器管理 |

## 共享运行时

`AppShell` 负责当前用户、可见媒体库、dashboard 布局和实时连接。实时事件只提示资源
revision 变化；React Query 根据资源键精准失效查询，重连时通过 realtime state 对账。
扫描进度使用服务端任务级权威值，通知内容通过原因码在本地翻译。

播放器核心位于 `components/media-player-panel/`。首页和媒体库列表共用
`LibrarySpotlightCard`，继续观看入口共用 `ContinueWatchingCard`。浮层优先复用
`GlassSelect`、共享 popover/modal surface 与 `HoverTooltip`，避免页面复制交互状态。

界面文案必须经过 `src/i18n/`。API 错误使用 `error_code + params` 本地化，仅在未知错误码
时展示服务端诊断 `message`。

## 本地运行

```bash
pnpm -C apps/mova-web install
pnpm -C apps/mova-web dev
```

开发服务器默认监听 `http://127.0.0.1:35173`，并把 `/api` 代理到
`MOVA_API_PROXY_TARGET`，默认值为 `http://127.0.0.1:36080`。

需要本地 UI 数据时可显式启用开发 mock：

```bash
VITE_MOVA_MOCK_API=true pnpm -C apps/mova-web dev
```

mock 只在 Vite 开发构建中可用，不是网络错误兜底，也不会让生产构建返回假数据。

## 验证

```bash
pnpm -C apps/mova-web check
pnpm -C apps/mova-web test
pnpm -C apps/mova-web build
```

Docker 部署、源码镜像构建和发布通道统一见根目录
[`../../README.md`](../../README.md)。
