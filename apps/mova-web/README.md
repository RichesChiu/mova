# mova-web

`mova-web` 是 Mova 的前端应用，基于 Vite、React、TypeScript、React Router、TanStack Query 和 SCSS。  
这份文档不重复接口契约，而是从代码入口、页面结构、共享组件、数据层和测试层来说明当前前端是怎么组织的。

当前产品 UI 文案先统一为英文，后续会在这套英文基线上补中英切换能力。

如果你要看接口字段和 HTTP/SSE 契约，优先看 [`../../docs/API.md`](../../docs/API.md)。

## 1. 入口与启动链路

| 文件 | 作用 |
| --- | --- |
| `src/main.tsx` | 浏览器入口。负责 `applyTheme()`、引入全局样式 `global.scss`、挂载 React 根节点。 |
| `src/App.tsx` | 应用入口。负责创建 `QueryClientProvider`、`BrowserRouter`，并声明完整路由树。 |
| `src/components/app-shell/index.tsx` | 登录后主壳层。负责查询当前用户、查询可见媒体库、挂载顶栏、处理登出、建立 SSE 连接，并把共享上下文下发给页面。 |
| `src/api/client.ts` | 前端统一 API 客户端。负责 `fetch`、错误处理、JSON envelope 解包，以及媒体流/字幕流 URL 构造。播放器这里也会通过它拿字幕列表和音轨列表，并在切换音轨时拼出带 `audio_track_id` 的播放地址。 |
| `src/api/types.ts` | 前后端共享的数据契约类型定义。页面和组件基本都依赖这里的 DTO。 |
| `src/lib/query-client.ts` | TanStack Query 的全局默认配置入口。 |
| `src/styles/global.scss` | 样式总入口，统一聚合 `_tokens.scss`、`_base.scss`、`_shared.scss` 和各 feature 的样式。 |

启动时实际链路是：

`main.tsx` -> `App.tsx` -> `AppShell` -> 对应页面 -> 页面内查询与共享组件

有一个例外：

- `/media-items/:mediaItemId/play` 走沉浸式播放器页，不挂在 `AppShell` 下面，因此它不会复用壳层布局，但仍然复用同一个 `QueryClientProvider` 和路由系统。

## 2. 当前前端架构

当前前端可以按 6 层来理解：

1. 启动层  
   `main.tsx`、`App.tsx`，负责主题、样式、路由和 Query Provider。

2. 壳层与会话层  
   `components/app-shell/` 负责登录态校验、媒体库列表、全局导航和 SSE 实时事件接入。

3. 页面层  
   `src/pages/` 下目前有 7 个路由页面目录，每个页面按 `index.tsx + *.scss` 组织。

4. 公用组件层  
   `src/components/` 下放跨页面复用的 UI 与交互组件，例如卡片、弹窗、播放器面板、滚动 rail、目录树等。

5. 数据与工具层  
   `src/api/` 负责 HTTP 契约；`src/lib/` 负责 Query 默认值、路由拼接、格式化、权限判断等纯工具。

6. 样式与测试层  
   `src/styles/` 管全局样式资产；测试基座在 `src/test/setup.ts`，具体测试文件跟着组件或页面放。

当前目录重点如下：

```text
src/
  main.tsx
  App.tsx
  api/
    client.ts
    types.ts
  components/
  lib/
  pages/
  styles/
  test/
```

## 3. 路由与页面

当前有 7 个路由页面目录，分别承担下面这些职责：

| 路由 | 页面文件 | 作用 | 主要数据来源 |
| --- | --- | --- | --- |
| `/login` | `src/pages/login-page/index.tsx` | 登录页和首个管理员 bootstrap 入口。根据 `bootstrap-status` 决定是“创建第一个管理员”还是普通登录。 | `getCurrentUser`、`getBootstrapStatus`、`login`、`bootstrapAdmin` |
| `/` | `src/pages/home-page/index.tsx` | 首页。聚合媒体库卡片、继续观看、各媒体库的 shelf；同时消费扫描实时态。 | `listLibraries`、`getLibrary`、`listLibraryMediaItems`、`listContinueWatching`、`getMediaItemEpisodeOutline` |
| `/libraries/:libraryId` | `src/pages/library-page/index.tsx` | 单库详情页。展示库标题、描述、关键统计、最新扫描状态、电影/剧集列表，以及扫描中的占位卡；页面头部会尽量保持内容页而不是配置页的层级。 | `getLibrary`、`listLibraryMediaItems`、`scanRuntimeByLibrary` |
| `/media-items/:mediaItemId` | `src/pages/media-item-page/index.tsx` | 媒体详情页。电影显示详情与播放入口，并在标题旁展示带 `IMDb` 标识的评分；hero 区会优先用背景图或海报做模糊大底与光晕层，让标题、海报和评分形成更完整的电影感头图，年份、国家/地区、题材类型和工作室则收在 facts 区里避免重复副标题；如果同一部电影存在多个本地版本，播放区会先给版本选择，技术信息区也会跟着当前版本切换；演员区下方会展示当前资源文件的 source details，以及并排的视频、音频、字幕技术卡；音轨和字幕卡头部都有小下拉，当前版本本身则只在播放区选择，字幕卡也会展示默认、强制、听障和外挂标记；剧集显示季/集大纲、演员和管理员元数据工具；当所在媒体库仍在扫描时，这里也会显示当前条目或当前季的同步状态与占位集卡。 | `getMediaItem`、`getMediaItemEpisodeOutline`、`getMediaItemPlaybackProgress`、`getMediaItemPlaybackHeader`、`listMediaItemFiles`、`listMediaFileAudioTracks`、`listMediaFileSubtitles`、`scanRuntimeByLibrary` |
| `/media-items/:mediaItemId/play` | `src/pages/media-player-page/index.tsx` | 沉浸式播放器页。负责装配播放器标题、副标题、集切换选项，并把实际播放行为交给 `MediaPlayerPanel`。 | `getMediaItemPlaybackHeader`、`getMediaItemEpisodeOutline` |
| `/profile` | `src/pages/profile-page/index.tsx` | 个人设置页。收成单块资料面板，展示用户名、昵称、角色标签，以及链接式的改密入口；昵称支持通过行内编辑 icon 直接更新，真正的密码修改仍在弹窗里完成。 | `updateOwnProfile`、`changeOwnPassword`、`AppShell` 提供的 `currentUser` |
| `/settings` | `src/pages/settings-page/index.tsx` | 管理员设置页。承接用户增删改查、媒体库创建、扫描、删除和基础配置编辑；危险操作会走统一确认弹窗。 | `listUsers`、`createUser`、`updateUser`、`deleteUser`、`createLibrary`、`updateLibrary`、`scanLibrary`、`deleteLibrary`、`getLibrary` |

几个页面内还有“页面级子模块”，但它们不算独立路由：

- `pages/home-page/libraries-section/`：首页顶部库聚焦卡
- `pages/home-page/continue-watching-section/`：继续观看区
- `pages/home-page/library-content-sections/`：首页各库的横向媒体 rail

## 4. 共享组件

当前 `src/components/` 下有 18 个已实现的公用组件目录；另外还有一个空的 `create-user-form/` 目录，当前还没有实现内容。

### 4.1 壳层与运行时

| 组件 | 文件 | 作用 | 主要使用位置 |
| --- | --- | --- | --- |
| `AppShell` | `components/app-shell/index.tsx` | 登录后壳层，负责当前用户、媒体库列表、SSE、顶栏和 `Outlet` 上下文。 | 所有非登录、非沉浸式播放器页面 |
| `useServerEvents` | `components/app-shell/use-server-events.ts` | 通过 `EventSource('/api/events')` 订阅 SSE，解析扫描/媒体库/元数据事件，并触发 React Query 刷新。 | `AppShell` |
| `scan-runtime` | `components/app-shell/scan-runtime.ts` | 把 SSE 运行时扫描数据整理成库级进度、条目级占位卡、详情页同步提示和状态文案。 | 首页、媒体库页、媒体详情页、设置页 |
| `ContentHeader` | `components/content-header/index.tsx` | 顶部品牌、未来 i18n 预留的语言下拉，以及用户菜单；顶栏用户区会优先显示昵称，没有昵称时回退到用户名。 | `AppShell` |

### 4.2 媒体展示

| 组件 | 文件 | 作用 | 主要使用位置 |
| --- | --- | --- | --- |
| `MediaCard` / `MediaCardSkeleton` / `MediaCardScanPlaceholder` | `components/media-card/index.tsx` | 统一的媒体卡片、骨架卡和扫描中占位卡；扫描态会尽量保持与最终卡片一致的占位尺寸，减少同步完成时的跳动，标题和扫描文案也会固定在更稳定的单行/单层布局里，避免首页库卡与媒体卡被长文案撑高。扫描中电影通常按文件展示，剧集则优先按系列目录组展示。 | 首页、媒体库页 |
| `EpisodeCard` / `EpisodeCardSkeleton` | `components/episode-card/index.tsx` | 统一的剧集卡片，支持可播放/不可播放状态和播放进度条。 | 媒体详情页 |
| `ScrollableRail` | `components/scrollable-rail/index.tsx` | 横向滚动容器，支持左右按钮、鼠标滚轮直接横滑、提示文案。 | 首页 rail、剧集页、演员区 |
| `MediaPlayerPanel` | `components/media-player-panel/index.tsx` | 真正的播放器核心组件，负责媒体源、字幕、音轨切换、播放进度、缓冲态、错误分类、非阻塞字幕/自动播放/全屏降级和集切换；音轨菜单也会给出当前选中状态、切换中提示和更友好的加载/失败文案；播放器会优先等可播放文件列表返回，播放进度查询不会再把整页长期卡在 `Loading player…`。 | `MediaPlayerPage` |

### 4.3 管理与编辑

| 组件 | 文件 | 作用 | 主要使用位置 |
| --- | --- | --- | --- |
| `CreateLibraryForm` | `components/create-library-form/index.tsx` | 建库表单，支持目录树选择、类型选择、元数据语言和启停；`Library Type` 和 `Root Path` 也会直接给出轻量提示，减少用户理解容器路径和库类型差异的成本。 | `CreateLibraryModal` |
| `CreateLibraryModal` | `components/create-library-modal/index.tsx` | 设置页里的建库弹窗，负责承接建库入口，不再把表单长期铺在页面底部。 | 设置页 |
| `LibraryEditorModal` | `components/library-editor-modal/index.tsx` | 编辑媒体库基础配置，当前支持名称、描述、元数据语言和启停状态；只读的库类型和根路径旁边也会给出同一套说明。 | 设置页 |
| `UserEditorModal` | `components/user-editor-modal/index.tsx` | 创建/编辑用户，支持用户名、昵称、角色、启停和媒体库授权。 | 设置页 |
| `ConfirmActionModal` | `components/confirm-action-modal/index.tsx` | 统一承接危险操作确认流和错误提示，当前用于删库、删用户。 | 设置页 |
| `ChangePasswordModal` | `components/change-password-modal/index.tsx` | 个人页的改密弹窗，统一处理当前密码校验、确认输入和错误反馈。 | 个人页 |
| `MetadataMatchPanel` | `components/metadata-match-panel/index.tsx` | 管理员手动搜索并替换单条媒体元数据。 | 媒体详情页 |
| `MediaDirectoryTree` | `components/media-directory-tree/index.tsx` | 递归目录树选择器，用于从容器内 `/media` 目录里选择库根路径。 | `CreateLibraryForm` |
| `GlassSelect` | `components/glass-select/index.tsx` | 自定义下拉选择器，统一风格与交互；也支持更紧凑的 compact 形态，用在详情页技术卡头部的小下拉。菜单会通过 portal 挂到根级浮层，避免被 hero、卡片或容器裁掉。 | 设置页、建库表单、用户编辑弹窗、媒体库编辑弹窗、媒体详情页 |

### 4.4 轻量 UI 基元

| 组件 | 文件 | 作用 | 主要使用位置 |
| --- | --- | --- | --- |
| `SectionHelp` | `components/section-help/index.tsx` | 节标题上的轻量 tooltip 帮助说明；tooltip 本体会通过 portal 挂到根级浮层，避免被卡片或容器裁掉。 | 需要补帮助说明的 section 标题 |
| `StatusPill` | `components/status-pill/index.tsx` | 把 `success / failed / neutral` 等文本状态渲染成统一 pill。 | 状态展示区 |
| `SettingsGearIcon` | `components/settings-gear-icon/index.tsx` | 设置相关的纯图标组件。 | 顶栏、设置页 hero |
| `MediaTypeTag` | `components/media-type-tag/index.tsx` | 把 `movie / series / episode` 渲染成更轻量的标签样式，避免视觉上像可点击按钮。 | 媒体卡、详情页等类型展示区 |

## 5. 数据层与共享工具

### `src/api/`

| 文件 | 作用 |
| --- | --- |
| `api/client.ts` | 统一封装所有 HTTP 请求、媒体文件流 URL、字幕流 URL，以及 API envelope 解包逻辑。 |
| `api/types.ts` | 前端所有 DTO 和请求体类型。页面和组件都依赖这里，而不是在本地重复声明接口。 |

### `src/lib/`

| 文件 | 作用 |
| --- | --- |
| `lib/query-client.ts` | 创建全局 `QueryClient`，统一 `retry`、`staleTime`、`refetchOnWindowFocus` 策略。 |
| `lib/query-options.ts` | 抽取媒体详情、剧集大纲等查询的缓存/过期常量。 |
| `lib/media-routes.ts` | 统一生成媒体详情页和播放页路径，避免各页面自己拼字符串。 |
| `lib/playback.ts` | 统一收口续播判断、播放入口链接、播放进度衍生状态，以及“接近片尾时视为已看完”的完成判定。 |
| `lib/audio-tracks.ts` | 统一音轨标签、语言和元信息文案，避免播放器菜单里散落格式化逻辑。 |
| `lib/media-country.ts` | 把 API 返回的国家/地区值整理成详情页可直接显示的文案；如果后端返回的是 ISO 国家码，这里会优先转成可读名称。 |
| `lib/media-file-details.ts` | 统一资源技术信息卡片里的视频、音频、字幕字段格式化，包括分辨率、码率、色彩参数、音轨/字幕标题，以及电影多版本切换所需的资源文件选项标签。 |
| `lib/player-feedback.ts` | 播放器兼容性提示文案，专门处理自动播放与全屏失败时的非阻断 warning。 |
| `lib/library-config.ts` | 统一媒体库编辑弹窗的 draft 初始化、变更判断和提交 payload 归一化。 |
| `lib/settings-admin.ts` | 收口设置页里的用户/媒体库缓存更新、扫描状态文案、本地占位 detail 构建，以及删库/删用户确认文案。 |
| `lib/user-identity.ts` | 统一昵称/用户名回退逻辑和用户头像首字母规则，供顶栏、设置页和个人页复用。 |
| `lib/viewer.ts` | 当前角色判断工具，决定哪些管理入口只给管理员看。 |
| `lib/format.ts` | 时间、日期、时长等显示格式化函数。 |
| `lib/theme.ts` | 启动时应用全局主题。 |

## 6. 样式与测试

### 样式

| 文件 | 作用 |
| --- | --- |
| `styles/global.scss` | 全局样式总入口。 |
| `styles/_tokens.scss` | 颜色、边框、阴影、尺寸等设计 token。 |
| `styles/_base.scss` | 基础元素和全局排版。 |
| `styles/_shared.scss` | 各页面复用的通用样式片段，例如骨架、布局和常见 panel。 |

当前前端没有使用 CSS Modules，而是走：

- 全局 SCSS 入口
- 组件/页面目录下各自的 `*.scss`
- 通过命名约定和 feature 目录来保持样式边界

### 测试

当前测试基座是：

- `Vitest`
- `@testing-library/react`
- `jsdom`
- `src/test/setup.ts`

已存在的测试文件包括：

- `components/app-shell/use-server-events.test.tsx`
- `components/app-shell/scan-runtime.test.ts`
- `components/media-player-panel/media-player-panel.test.tsx`
- `lib/audio-tracks.test.ts`
- `lib/media-country.test.ts`
- `lib/media-file-details.test.ts`
- `lib/playback.test.ts`
- `lib/player-feedback.test.ts`
- `lib/library-config.test.ts`
- `lib/settings-admin.test.ts`

当前这些测试重点覆盖：

- `useServerEvents` 的断线恢复、媒体库删除跳转、媒体库更新刷新、元数据更新刷新，以及扫描运行时状态保持
- `scan-runtime` 的扫描中文案、占位显示、详情页条目匹配和粗粒度进度计算
- `MediaPlayerPanel` 的恢复播放、从头播放、切源迁移、音轨切换时的位置保持、切换提示文案、错误文案映射，以及自动播放/全屏失败与字幕失败的非阻断降级
- `audio-tracks` helper 的音轨菜单标签和元信息格式化
- `media-country` helper 的国家/地区格式化与 ISO 国家码显示
- `media-file-details` helper 的视频/音频/字幕技术卡字段格式化、头部下拉选项、码率显示和杜比标记识别
- `playback` helper 的续播判定、默认播放入口、剧集优先选择和接近片尾的完成判定
- `library-config` helper 的 draft 初始化、变更判断和提交 payload 归一化
- `settings-admin` helper 的设置页本地缓存更新、扫描状态摘要、确认文案，以及删除/更新/启停后的边界收口

测试策略上，当前更偏向：

- 保留高风险 hook 和播放器交互测试
- 把页面按钮、占位文案、表单 payload 这类逻辑尽量下沉到纯函数，用 `.test.ts` 覆盖
- 避免堆太多页面渲染级 `tsx` 测试，减少样式和文案微调带来的维护成本

## 7. 运行

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

## 8. Docker

根目录执行：

```bash
docker compose up -d --build
```

构建后的前端静态文件会进入 `mova-server` 镜像，由后端直接托管；运行时继续走同域 `/api/*`。

如果你要看更完整的 MVP 阶段部署和升级说明，见 [`../../docs/DEPLOYMENT.md`](../../docs/DEPLOYMENT.md)。

## 9. 质量工具

```bash
pnpm test
pnpm format
pnpm lint
pnpm check
```
