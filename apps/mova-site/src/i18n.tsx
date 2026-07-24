import { useEffect, useMemo, useState, type ReactNode } from 'react'
import { I18nContext, type I18nContextValue, type Language } from './i18n-context'

const translations: Record<string, string> = {
  首页: 'Home',
  部署: 'Deploy',
  'API 文档': 'API Docs',
  '返回 MOVA 首页': 'Back to MOVA home',
  主要导航: 'Main navigation',
  '打开 GitHub 仓库': 'Open the GitHub repository',
  '打开 Docker 镜像': 'Open the Docker image',
  '切换为英文': 'Switch to English',
  '切换为中文': 'Switch to Chinese',
  首屏操作: 'Hero actions',
  'MOVA 网页端媒体库首页界面': 'MOVA Web media library home screen',
  属于你自己的: 'Your own',
  流媒体中心: 'streaming center',
  'MOVA 是美观、好用的自托管流媒体服务器': 'MOVA is a beautiful, easy-to-use self-hosted streaming server',
  '集中管理本地电影和剧集，通过网页随时访问，原生客户端持续开发中。':
    'Organize local movies and series, access them on the Web, and follow the native clients as they evolve.',
  开始部署: 'Deploy now',
  '查看 API': 'View API',
  'MOVA 核心优势': 'MOVA core benefits',
  隐私优先: 'Privacy first',
  '媒体和账户数据始终由你掌控。': 'Your media and account data stay under your control.',
  开源可信: 'Open and trustworthy',
  '完整开源透明，安全可审阅。': 'Fully open source, transparent, and auditable.',
  跨设备访问: 'Cross-device access',
  '网页与 macOS 随时访问媒体库。': 'Access your library on the Web and macOS.',
  持续进化: 'Always evolving',
  '社区与作者持续完善产品体验。': 'The community and author keep improving the experience.',
  '强大功能，': 'Powerful features,',
  全面掌控你的媒体: 'complete control of your media',
  'MOVA 核心功能': 'MOVA core capabilities',
  私有媒体库: 'Private media library',
  '集中管理本地电影和剧集，本地文件只读挂载，不改动原始媒体。':
    'Organize local movies and series while mounting media read-only and leaving original files untouched.',
  多设备访问: 'Multi-device access',
  '手机、平板、电脑和电视都能通过 Web 访问，随时随地继续观看。':
    'Continue watching anywhere through the Web on phones, tablets, computers, and TVs.',
  高效媒体解析: 'Efficient media analysis',
  '配合 ffprobe 识别 4K、HDR、Dolby Vision、Atmos 等资源级标签。':
    'Use ffprobe to identify asset-level tags such as 4K, HDR, Dolby Vision, and Atmos.',
  用户与权限管理: 'Users and permissions',
  '首次启动创建管理员，后续可按家庭和设备场景管理访问边界。':
    'Create an administrator on first launch, then manage access for household and device scenarios.',
  元数据整理: 'Metadata enrichment',
  '按文件名归组电影与剧集，通过 TMDB 补齐海报、背景图、标题 Logo 和评分。':
    'Group movies and series by filename, then use TMDB to enrich posters, backdrops, title logos, and ratings.',
  'macOS 客户端': 'macOS client',
  'macOS 平台说明': 'macOS platform details',
  'MOVA macOS 原生客户端详情界面': 'MOVA native macOS client detail screen',
  '专为 macOS 打造的': 'Designed for macOS',
  原生体验: 'A native experience',
  '原生 macOS 客户端即将推出。': 'The native macOS client is coming soon.',
  即将到来: 'Coming soon',
  '尚未上架 Mac App Store': 'Not yet available on the Mac App Store',
  跨平台支持: 'Cross-platform support',
  '在你常用的设备上，随时访问你的媒体库':
    'Access your media library anytime on the devices you use most.',
  'MOVA 平台状态': 'MOVA platform availability',
  现在即可使用: 'Available now',
  敬请期待: 'Stay tuned',
  网页端: 'Web',
  '在浏览器中随时访问，无需安装。': 'Access it anytime in your browser, with nothing to install.',
  'macOS 端': 'macOS',
  'Mac App Store 即将上架': 'Coming soon to the Mac App Store',
  'iOS 端': 'iOS',
  'iOS 客户端，敬请期待。': 'The iOS client is on the way. Stay tuned.',
  'Pad 端': 'Pad',
  'Pad 客户端，敬请期待。': 'The Pad client is on the way. Stay tuned.',
  'MOVA API 文档': 'MOVA API Documentation',

  '根据服务端文档整理当前 mova-server 已实现的 HTTP 接口，覆盖鉴权、媒体库扫描、媒体条目、播放进度、媒体流和播放器接入需要的 ID 流转。':
    'A reference to the HTTP endpoints currently implemented by mova-server, covering authentication, library scanning, media items, playback progress, streaming, and the ID flow required by players.',
  查看部署方式: 'View deployment guide',
  返回首页: 'Back to home',
  已整理接口: 'Documented endpoints',
  接口分组: 'Endpoint groups',
  'GET 接口': 'GET endpoints',
  登录方式: 'Authentication methods',
  'API 摘要': 'API summary',
  'API 文档内容': 'API documentation content',
  完整细节请以项目文档为准: 'Use the project documentation as the source of truth',
  '完整 API.md': 'Complete API.md',
  '完整 SSE.md': 'Complete SSE.md',
  'MOVA 项目仓库': 'MOVA repository',
  文档目录: 'Contents',
  通用说明: 'General',
  'ID 关系': 'ID relationships',
  关键规则: 'Key rules',
  常见状态码: 'Common status codes',
  成功响应: 'Success response',
  错误响应: 'Error response',
  'ID 关系与播放流转': 'ID relationships and playback flow',
  '前端接入播放器时最容易混淆的是媒体库、媒体条目、媒体文件、音轨和字幕的 ID。下面按使用顺序整理一遍。':
    'When integrating a player, library, media item, media file, audio track, and subtitle IDs are easy to confuse. The sequence below shows how they flow through playback.',
  '本地默认服务地址，部署后替换为你的服务器域名。':
    'The default local service address. Replace it with your server domain after deployment.',
  响应格式: 'Response format',
  '业务接口统一 JSON envelope，媒体流和图片资源直接返回文件流。':
    'Business endpoints use a consistent JSON envelope, while media and image resources return file streams directly.',
  登录态: 'Authentication',
  'Web 使用 session cookie，原生客户端使用 token-login 返回的 Bearer token。':
    'The Web app uses a session cookie; native clients use the Bearer token returned by token-login.',
  实时事件: 'Realtime events',
  'GET /api/realtime/events 推送资源失效与临时扫描进度。':
    'GET /api/realtime/events pushes resource invalidation and transient scan progress.',
  'health、bootstrap-status、bootstrap-admin、login、token-login 和 refresh 可匿名访问，其余接口都要求登录态。':
    'health, bootstrap-status, bootstrap-admin, login, token-login, and refresh are public; all other endpoints require authentication.',
  '用户管理、建库、删库、触发扫描、服务器根目录等管理类接口要求 admin 权限。':
    'Administrative endpoints for users, libraries, scans, and server roots require admin permission.',
  'Web 端使用 session cookie；原生客户端使用 access token，refresh token 仅用于调用 refresh 接口。':
    'The Web app uses a session cookie; native clients use an access token, and the refresh token is only used by the refresh endpoint.',
  'realtime/events 返回 text/event-stream，不使用统一 JSON envelope；重连后应先请求 realtime/state。':
    'realtime/events returns text/event-stream instead of the JSON envelope; request realtime/state first after reconnecting.',
  '媒体条目图片 URL 会带版本参数，浏览器可长期缓存；元数据更新后版本会变化。':
    'Media image URLs include a version parameter for long-lived browser caching; the version changes after metadata updates.',
  '认证错误可能使用 TOKEN_EXPIRED、TOKEN_INVALID 或 REFRESH_TOKEN_INVALID 等字符串 code，客户端应按 code 处理重新登录或刷新。':
    'Authentication errors may use string codes such as TOKEN_EXPIRED, TOKEN_INVALID, or REFRESH_TOKEN_INVALID; clients should use the code to refresh credentials or sign in again.',
  'TMDB token 来自 MOVA_TMDB_ACCESS_TOKEN；当前评分来源仅接入 TMDB，其他外部 ID 只用于跨来源识别。':
    'The TMDB token comes from MOVA_TMDB_ACCESS_TOKEN; ratings currently come only from TMDB, while other external IDs are stored only for cross-provider identity.',
  'OK，请求成功': 'OK, request succeeded',
  'Created，创建成功': 'Created successfully',
  'Accepted，异步任务已创建': 'Accepted, asynchronous task created',
  'Bad Request，参数或业务校验失败': 'Bad Request, parameter or business validation failed',
  'Unauthorized，未登录或会话失效': 'Unauthorized, not signed in or session expired',
  'Forbidden，权限不足': 'Forbidden, insufficient permission',
  'Not Found，资源不存在': 'Not Found, resource does not exist',
  'Conflict，当前资源状态不允许操作': 'Conflict, the current resource state does not allow this operation',
  'Range Not Satisfiable，媒体 Range 越界': 'Range Not Satisfiable, media range is out of bounds',
  'Internal Server Error，服务内部错误': 'Internal Server Error',

  健康检查: 'Health',
  '用于探测服务进程和数据库是否可用，适合容器探针、本地调试和部署后的联通性检查。':
    'Checks service and database availability for container probes, local debugging, and post-deployment connectivity tests.',
  匿名可访问: 'Publicly accessible',
  '成功时返回 { "status": "ok" }': 'Returns { "status": "ok" } on success',
  适合作为部署后第一条检查接口: 'A good first check after deployment',
  '认证、用户与实时同步': 'Authentication, users, and realtime sync',
  '覆盖首次初始化、Cookie / Bearer 登录、Token 轮换、首页快照、资源 revision、SSE 和管理员用户管理。':
    'Covers first-time setup, Cookie/Bearer login, token rotation, home snapshots, resource revisions, SSE, and administrator user management.',
  'bootstrap 只在系统没有管理员时允许创建首个 admin，并直接建立登录态。':
    'bootstrap can create the first admin only when none exists, and immediately establishes an authenticated session.',
  'token-login 返回短期 access token 和长期 refresh token，refresh 会轮换两者。':
    'token-login returns a short-lived access token and a long-lived refresh token; refresh rotates both.',
  '/api/home 返回当前用户的有界首页快照，并携带 realtime revision 基线。':
    '/api/home returns a bounded home snapshot for the current user with the realtime revision baseline.',
  'SSE 只承载资源失效与临时进度；断线恢复必须使用 /api/realtime/state。':
    'SSE carries resource invalidation and transient progress only; reconnect recovery must use /api/realtime/state.',
  查询是否需要初始化首个管理员: 'Check whether the first administrator must be initialized',
  初始化首个管理员并登录: 'Initialize the first administrator and sign in',
  登录: 'Sign in',
  '为原生客户端创建 access token 和 refresh token': 'Create access and refresh tokens for native clients',
  '使用 refresh token 轮换并获取新的 token': 'Rotate tokens with a refresh token',
  登出: 'Sign out',
  查询当前用户: 'Get the current user',
  更新当前用户昵称: 'Update the current user display name',
  查询当前用户的轻量首页快照: 'Get the current user lightweight home snapshot',
  查询当前可见资源版本和活跃扫描: 'Get visible resource versions and active scans',
  '订阅资源失效与临时扫描进度（SSE）': 'Subscribe to resource invalidation and transient scan progress (SSE)',
  当前用户修改自己的密码: 'Change the current user password',
  '查询用户列表（管理员）': 'List users (admin)',
  '创建用户（管理员）': 'Create a user (admin)',
  '更新低权限用户的角色与媒体库权限（管理员）':
    'Update roles and library access for lower-privilege users (admin)',
  '删除用户（管理员）': 'Delete a user (admin)',
  管理员重置指定用户密码: 'Reset a user password (admin)',
  通知中心: 'Notifications',
  '返回当前用户可见的持久化通知、总未读数和分类未读数，并支持单条或批量标记已读。':
    'Returns persistent notifications visible to the current user, total and category unread counts, and supports marking one or many as read.',
  '标准类别包括 scan、system、library 和 account，未知类别也必须保留展示。':
    'Standard categories include scan, system, library, and account; unknown categories must remain visible.',
  '通知和已读状态持久化在 PostgreSQL，SSE 只通知客户端重新读取。':
    'Notifications and read states are persisted in PostgreSQL; SSE only tells clients to fetch again.',
  'GET 响应的未读统计不受 category 筛选影响。':
    'Unread counts in GET responses are not affected by the category filter.',
  '标记已读操作幂等，只有状态首次变化时才推进 revision。':
    'Mark-as-read operations are idempotent; the revision advances only on the first state change.',
  查询当前用户可见的通知和分类未读数: 'Get visible notifications and category unread counts',
  批量标记当前用户的通知为已读: 'Mark multiple notifications as read',
  标记一条可见通知为已读: 'Mark one visible notification as read',
  服务器媒体目录: 'Server media directories',
  '供管理员查询容器内当前可用于建库的媒体文件夹树。':
    'Lets administrators inspect the media folder tree available for creating libraries inside the container.',
  '仅 admin 可访问。': 'Admin access only.',
  '只返回文件夹，不返回普通文件。': 'Returns folders only, not regular files.',
  '返回的 path 可直接用作创建媒体库的 root_path。':
    'The returned path can be used directly as the root_path for a media library.',
  '客户端不得把本机文件系统路径作为服务端 root_path。':
    'Clients must not send a local filesystem path as the server root_path.',
  查询服务端当前可用于建库的媒体文件夹树: 'Get the server media folder tree available for libraries',
  媒体库与搜索: 'Libraries and search',
  '围绕媒体库配置、最新添加、列表详情、扫描历史、异步扫描和全局搜索展开。':
    'Covers library configuration, recently added items, details, scan history, asynchronous scanning, and global search.',
  '媒体库统一自动识别电影和剧集，不再要求用户手动选择库类型。':
    'Libraries automatically identify movies and series without requiring users to choose a library type.',
  'metadata_language 支持 zh-CN / en-US，影响扫描和 TMDB 元数据补全语言。':
    'metadata_language supports zh-CN and en-US and controls scanning and TMDB metadata language.',
  '创建媒体库后会自动触发一次后台扫描；媒体库不提供启用/禁用状态。':
    'Creating a library automatically starts a background scan; libraries do not have an enabled/disabled state.',
  '删除媒体库会由数据库级联清理权威数据，并持久化后台任务删除该库独立的图片、字幕和音轨缓存。':
    'Deleting a library cascades its authoritative database data and persists a background job that removes the library-scoped artwork, subtitle, and audio caches.',
  '搜索会在当前用户可见库内匹配电影、剧集和本地可用的集条目。':
    'Search matches movies, series, and locally available episodes in libraries visible to the current user.',
  查询媒体库列表: 'List media libraries',
  查询按库分组的最新添加内容: 'Get recently added content grouped by library',
  创建媒体库: 'Create a media library',
  查询单个媒体库详情: 'Get media library details',
  更新媒体库基础配置: 'Update media library configuration',
  删除媒体库: 'Delete a media library',
  查询媒体库下的媒体条目列表: 'List media items in a library',
  查询媒体库扫描历史: 'Get media library scan history',
  查询单个扫描任务状态: 'Get scan job status',
  触发异步扫描: 'Start an asynchronous scan',
  '搜索当前用户可见库下的电影、剧集和集条目': 'Search movies, series, and episodes in visible libraries',
  媒体条目: 'Media items',
  '提供电影、剧集、季、集、演员、播放头、文件列表、元数据匹配与图片资源读取。':
    'Provides movies, series, seasons, episodes, cast, playback headers, file lists, metadata matching, and image resources.',
  'media_item_id 不是 library_id；详情、文件列表、播放进度都围绕 media_item_id 展开。':
    'media_item_id is not library_id; details, file lists, and playback progress all use media_item_id.',
  'metadata_status 使用 matched / unmatched / failed / skipped 表达元数据处理状态。':
    'metadata_status uses matched, unmatched, failed, and skipped to represent metadata processing state.',
  '剧集可通过 seasons、episodes、episode-outline 获取本地可用集和远端大纲合并结果。':
    'Series use seasons, episodes, and episode-outline to merge locally available episodes with remote outlines.',
  'poster/backdrop 返回图片流；若详情字段是远程 URL，前端可直接使用远程地址。':
    'poster/backdrop return image streams; when a detail field is a remote URL, clients may use it directly.',
  查询单个媒体条目详情: 'Get media item details',
  查询单个媒体条目的演员列表: 'Get the cast for a media item',
  查询播放器页头部信息: 'Get player header information',
  查询媒体条目关联文件列表: 'Get files associated with a media item',
  查询剧集全集大纲并标记本地可用集: 'Get a complete series outline with local availability',
  '手动搜索单条媒体的候选元数据（管理员）': 'Search metadata candidates for one media item (admin)',
  '选择候选结果并替换当前媒体元数据（管理员）': 'Select a candidate and replace current metadata (admin)',
  手动重拉单个媒体条目元数据: 'Refresh metadata for one media item',
  读取媒体条目海报图: 'Read a media item poster',
  读取媒体条目背景图: 'Read a media item backdrop',
  读取某一季海报图: 'Read a season poster',
  读取某一季背景图: 'Read a season backdrop',
  播放进度: 'Playback progress',
  '记录当前用户的播放位置和继续观看列表，所有进度都按登录用户隔离。':
    'Stores playback position and continue-watching state for the current user, isolated per account.',
  '查询进度返回 null 是正常语义，表示当前用户尚未观看该内容。':
    'A null progress response is normal and means the current user has not watched the item.',
  '写入进度时同时提交 media_file_id、position_seconds 和 duration_seconds。':
    'Progress updates include media_file_id, position_seconds, and duration_seconds.',
  'continue-watching 只返回未看完内容，剧集会按 series 聚合到最近观看的一集。':
    'continue-watching returns unfinished items only and groups series by the most recently watched episode.',
  '已看完内容不会出现在继续观看列表中。': 'Completed items do not appear in continue watching.',
  查询单条内容的最近播放进度: 'Get recent playback progress for an item',
  写入或更新播放进度: 'Create or update playback progress',
  查询继续观看列表: 'Get the continue-watching list',
  媒体流: 'Media streams',
  '播放器相关接口：内嵌音轨、字幕列表、WebVTT 字幕输出、媒体文件流和 HEAD 探测。':
    'Player endpoints for embedded audio tracks, subtitle lists, WebVTT output, media streams, and HEAD probes.',
  '媒体流和字幕流不返回 JSON envelope，直接返回文件流或 text/vtt。':
    'Media and subtitle streams do not use the JSON envelope; they return file streams or text/vtt directly.',
  'GET /stream 支持 Range 请求，拖动进度条时通常返回 206 Partial Content。':
    'GET /stream supports Range requests and usually returns 206 Partial Content when seeking.',
  'audio_track_id 会触发后端验证并生成 remux 缓存变体，这不是多码率转码。':
    'audio_track_id triggers validation and creates a cached remux variant; this is not adaptive-bitrate transcoding.',
  '字幕接口会把 srt、ass/ssa、内嵌字幕统一转换成浏览器可挂载的 WebVTT。':
    'Subtitle endpoints convert srt, ass/ssa, and embedded subtitles to browser-ready WebVTT.',
  查询媒体文件可切换的内嵌音轨列表: 'List selectable embedded audio tracks',
  查询媒体文件可切换字幕列表: 'List selectable subtitles',
  '输出单条字幕轨道的 WebVTT 内容': 'Output one subtitle track as WebVTT',
  播放媒体文件: 'Stream a media file',
  查询媒体文件播放头信息: 'Get media file playback headers',
  '来自 /api/libraries，用于媒体库相关接口': 'From /api/libraries; used by library endpoints',
  '来自媒体库 media-items，用于详情、文件列表和播放进度':
    'From library media-items; used for details, file lists, and playback progress',
  '来自 /api/media-items/{id}/files，用于播放媒体流和进度上报':
    'From /api/media-items/{id}/files; used for streaming and progress reporting',
  '来自 /api/media-files/{id}/audio-tracks，用于切换内嵌音轨':
    'From /api/media-files/{id}/audio-tracks; used to switch embedded audio tracks',
  '来自 /api/media-files/{id}/subtitles，用于加载单条字幕轨道':
    'From /api/media-files/{id}/subtitles; used to load one subtitle track',
}

const getInitialLanguage = (): Language => {
  if (typeof window === 'undefined') {
    return 'zh'
  }

  return window.localStorage.getItem('mova-language') === 'en' ? 'en' : 'zh'
}

export function I18nProvider({ children }: { children: ReactNode }) {
  const [language, setLanguage] = useState<Language>(getInitialLanguage)

  useEffect(() => {
    window.localStorage.setItem('mova-language', language)
    document.documentElement.lang = language === 'zh' ? 'zh-CN' : 'en'
  }, [language])

  const value = useMemo<I18nContextValue>(
    () => ({
      language,
      setLanguage,
      t: (text) => (language === 'en' ? translations[text] ?? text : text),
    }),
    [language],
  )

  return <I18nContext.Provider value={value}>{children}</I18nContext.Provider>
}
