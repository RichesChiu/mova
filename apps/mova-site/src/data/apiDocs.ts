export type HttpMethod = 'GET' | 'POST' | 'PATCH' | 'PUT' | 'DELETE' | 'HEAD'

export type ApiEndpoint = {
  method: HttpMethod
  path: string
  description: string
}

export type ApiEndpointGroup = {
  id: string
  title: string
  summary: string
  highlights: string[]
  endpoints: ApiEndpoint[]
}

export const apiOverviewCards = [
  {
    label: 'Base URL',
    value: 'http://127.0.0.1:36080',
    text: '本地默认服务地址，部署后替换为你的服务器域名。',
  },
  {
    label: '响应格式',
    value: 'code / message / data',
    text: '业务接口统一 JSON envelope，媒体流和图片资源直接返回文件流。',
  },
  {
    label: '登录态',
    value: 'Cookie / Bearer',
    text: 'Web 使用 session cookie，原生客户端使用 token-login 返回的 Bearer token。',
  },
  {
    label: '实时事件',
    value: 'text/event-stream',
    text: 'GET /api/realtime/events 推送资源失效与临时扫描进度。',
  },
]

export const apiCommonNotes = [
  'health、bootstrap-status、bootstrap-admin、login、token-login 和 refresh 可匿名访问，其余接口都要求登录态。',
  '用户管理、建库、删库、触发扫描、服务器根目录等管理类接口要求 admin 权限。',
  'Web 端使用 session cookie；原生客户端使用 access token，refresh token 仅用于调用 refresh 接口。',
  'realtime/events 返回 text/event-stream，不使用统一 JSON envelope；重连后应先请求 realtime/state。',
  '媒体条目图片 URL 会带版本参数，浏览器可长期缓存；元数据更新后版本会变化。',
  '认证错误可能使用 TOKEN_EXPIRED、TOKEN_INVALID 或 REFRESH_TOKEN_INVALID 等字符串 code，客户端应按 code 处理重新登录或刷新。',
  'TMDB token 来自 MOVA_TMDB_ACCESS_TOKEN；当前评分来源仅接入 TMDB，其他外部 ID 只用于跨来源识别。',
]

export const apiSourceLinks = {
  repository: 'https://github.com/RichesChiu/mova',
  api: 'https://github.com/RichesChiu/mova/blob/master/docs/API.md',
  sse: 'https://github.com/RichesChiu/mova/blob/master/docs/SSE.md',
}

export const apiStatusCodes = [
  ['200', 'OK，请求成功'],
  ['201', 'Created，创建成功'],
  ['202', 'Accepted，异步任务已创建'],
  ['400', 'Bad Request，参数或业务校验失败'],
  ['401', 'Unauthorized，未登录或会话失效'],
  ['403', 'Forbidden，权限不足'],
  ['404', 'Not Found，资源不存在'],
  ['409', 'Conflict，当前资源状态不允许操作'],
  ['416', 'Range Not Satisfiable，媒体 Range 越界'],
  ['500', 'Internal Server Error，服务内部错误'],
]

export const apiSuccessExample = `{
  "code": 200,
  "message": "ok",
  "data": {
    "...": "..."
  }
}`

export const apiErrorExample = `{
  "code": 404,
  "message": "resource not found",
  "data": null
}`

export const apiEndpointGroups: ApiEndpointGroup[] = [
  {
    id: 'health',
    title: '健康检查',
    summary: '用于探测服务进程和数据库是否可用，适合容器探针、本地调试和部署后的联通性检查。',
    highlights: ['匿名可访问', '成功时返回 { "status": "ok" }', '适合作为部署后第一条检查接口'],
    endpoints: [{ method: 'GET', path: '/api/health', description: '健康检查' }],
  },
  {
    id: 'auth-realtime',
    title: '认证、用户与实时同步',
    summary: '覆盖首次初始化、Cookie / Bearer 登录、Token 轮换、首页快照、资源 revision、SSE 和管理员用户管理。',
    highlights: [
      'bootstrap 只在系统没有管理员时允许创建首个 admin，并直接建立登录态。',
      'token-login 返回短期 access token 和长期 refresh token，refresh 会轮换两者。',
      '/api/home 返回当前用户的有界首页快照，并携带 realtime revision 基线。',
      'SSE 只承载资源失效与临时进度；断线恢复必须使用 /api/realtime/state。',
    ],
    endpoints: [
      { method: 'GET', path: '/api/auth/bootstrap-status', description: '查询是否需要初始化首个管理员' },
      { method: 'POST', path: '/api/auth/bootstrap-admin', description: '初始化首个管理员并登录' },
      { method: 'POST', path: '/api/auth/login', description: '登录' },
      { method: 'POST', path: '/api/auth/token-login', description: '为原生客户端创建 access token 和 refresh token' },
      { method: 'POST', path: '/api/auth/refresh', description: '使用 refresh token 轮换并获取新的 token' },
      { method: 'POST', path: '/api/auth/logout', description: '登出' },
      { method: 'GET', path: '/api/auth/me', description: '查询当前用户' },
      { method: 'PATCH', path: '/api/auth/me', description: '更新当前用户昵称' },
      { method: 'GET', path: '/api/home', description: '查询当前用户的轻量首页快照' },
      { method: 'GET', path: '/api/realtime/state', description: '查询当前可见资源版本和活跃扫描' },
      { method: 'GET', path: '/api/realtime/events', description: '订阅资源失效与临时扫描进度（SSE）' },
      { method: 'PUT', path: '/api/auth/password', description: '当前用户修改自己的密码' },
      { method: 'GET', path: '/api/users', description: '查询用户列表（管理员）' },
      { method: 'POST', path: '/api/users', description: '创建用户（管理员）' },
      { method: 'PATCH', path: '/api/users/{id}', description: '更新用户基础信息（管理员）' },
      { method: 'DELETE', path: '/api/users/{id}', description: '删除用户（管理员）' },
      { method: 'PUT', path: '/api/users/{id}/password', description: '管理员重置指定用户密码' },
    ],
  },
  {
    id: 'notifications',
    title: '通知中心',
    summary: '返回当前用户可见的持久化通知、总未读数和分类未读数，并支持单条或批量标记已读。',
    highlights: [
      '标准类别包括 scan、system、library 和 account，未知类别也必须保留展示。',
      '通知和已读状态持久化在 PostgreSQL，SSE 只通知客户端重新读取。',
      'GET 响应的未读统计不受 category 筛选影响。',
      '标记已读操作幂等，只有状态首次变化时才推进 revision。',
    ],
    endpoints: [
      { method: 'GET', path: '/api/notifications', description: '查询当前用户可见的通知和分类未读数' },
      { method: 'PUT', path: '/api/notifications', description: '批量标记当前用户的通知为已读' },
      { method: 'PUT', path: '/api/notifications/{id}/read', description: '标记一条可见通知为已读' },
    ],
  },
  {
    id: 'server-media',
    title: '服务器媒体目录',
    summary: '供管理员查询容器内当前可用于建库的媒体文件夹树。',
    highlights: [
      '仅 admin 可访问。',
      '只返回文件夹，不返回普通文件。',
      '返回的 path 可直接用作创建媒体库的 root_path。',
      '客户端不得把本机文件系统路径作为服务端 root_path。',
    ],
    endpoints: [
      { method: 'GET', path: '/api/server/media-tree', description: '查询服务端当前可用于建库的媒体文件夹树' },
    ],
  },
  {
    id: 'libraries',
    title: '媒体库与搜索',
    summary: '围绕媒体库配置、最新添加、列表详情、扫描历史、异步扫描和全局搜索展开。',
    highlights: [
      '媒体库统一自动识别电影和剧集，不再要求用户手动选择库类型。',
      'metadata_language 支持 zh-CN / en-US，影响扫描和 TMDB 元数据补全语言。',
      '创建媒体库后会自动触发一次后台扫描；媒体库不提供启用/禁用状态。',
      '搜索会在当前用户可见库内匹配电影、剧集和本地可用的集条目。',
    ],
    endpoints: [
      { method: 'GET', path: '/api/libraries', description: '查询媒体库列表' },
      { method: 'GET', path: '/api/libraries/recently-added', description: '查询按库分组的最新添加内容' },
      { method: 'POST', path: '/api/libraries', description: '创建媒体库' },
      { method: 'GET', path: '/api/libraries/{id}', description: '查询单个媒体库详情' },
      { method: 'PATCH', path: '/api/libraries/{id}', description: '更新媒体库基础配置' },
      { method: 'DELETE', path: '/api/libraries/{id}', description: '删除媒体库' },
      { method: 'GET', path: '/api/libraries/{id}/media-items', description: '查询媒体库下的媒体条目列表' },
      { method: 'GET', path: '/api/libraries/{id}/scan-jobs', description: '查询媒体库扫描历史' },
      { method: 'GET', path: '/api/libraries/{id}/scan-jobs/{scan_job_id}', description: '查询单个扫描任务状态' },
      { method: 'POST', path: '/api/libraries/{id}/scan', description: '触发异步扫描' },
      { method: 'GET', path: '/api/search', description: '搜索当前用户可见库下的电影、剧集和集条目' },
    ],
  },
  {
    id: 'media-items',
    title: '媒体条目',
    summary: '提供电影、剧集、季、集、演员、播放头、文件列表、元数据匹配与图片资源读取。',
    highlights: [
      'media_item_id 不是 library_id；详情、文件列表、播放进度都围绕 media_item_id 展开。',
      'metadata_status 使用 matched / unmatched / failed / skipped 表达元数据处理状态。',
      '剧集可通过 seasons、episodes、episode-outline 获取本地可用集和远端大纲合并结果。',
      'poster/backdrop/logo 返回图片流；若详情字段是远程 URL，前端可直接使用远程地址。',
    ],
    endpoints: [
      { method: 'GET', path: '/api/media-items/{id}', description: '查询单个媒体条目详情' },
      { method: 'GET', path: '/api/media-items/{id}/cast', description: '查询单个媒体条目的演员列表' },
      { method: 'GET', path: '/api/media-items/{id}/playback-header', description: '查询播放器页头部信息' },
      { method: 'GET', path: '/api/media-items/{id}/files', description: '查询媒体条目关联文件列表' },
      { method: 'GET', path: '/api/media-items/{id}/episode-outline', description: '查询剧集全集大纲并标记本地可用集' },
      { method: 'GET', path: '/api/media-items/{id}/metadata-search', description: '手动搜索单条媒体的候选元数据（管理员）' },
      { method: 'POST', path: '/api/media-items/{id}/metadata-match', description: '选择候选结果并替换当前媒体元数据（管理员）' },
      { method: 'POST', path: '/api/media-items/{id}/refresh-metadata', description: '手动重拉单个媒体条目元数据' },
      { method: 'GET', path: '/api/media-items/{id}/poster', description: '读取媒体条目海报图' },
      { method: 'GET', path: '/api/media-items/{id}/backdrop', description: '读取媒体条目背景图' },
      { method: 'GET', path: '/api/media-items/{id}/logo', description: '读取媒体条目透明标题 Logo' },
      { method: 'GET', path: '/api/seasons/{id}/poster', description: '读取某一季海报图' },
      { method: 'GET', path: '/api/seasons/{id}/backdrop', description: '读取某一季背景图' },
    ],
  },
  {
    id: 'playback',
    title: '播放进度',
    summary: '记录当前用户的播放位置和继续观看列表，所有进度都按登录用户隔离。',
    highlights: [
      '查询进度返回 null 是正常语义，表示当前用户尚未观看该内容。',
      '写入进度时同时提交 media_file_id、position_seconds 和 duration_seconds。',
      'continue-watching 只返回未看完内容，剧集会按 series 聚合到最近观看的一集。',
      '已看完内容不会出现在继续观看列表中。',
    ],
    endpoints: [
      { method: 'GET', path: '/api/media-items/{id}/playback-progress', description: '查询单条内容的最近播放进度' },
      { method: 'PUT', path: '/api/media-items/{id}/playback-progress', description: '写入或更新播放进度' },
      { method: 'GET', path: '/api/playback-progress/continue-watching', description: '查询继续观看列表' },
    ],
  },
  {
    id: 'streams',
    title: '媒体流',
    summary: '播放器相关接口：内嵌音轨、字幕列表、WebVTT 字幕输出、媒体文件流和 HEAD 探测。',
    highlights: [
      '媒体流和字幕流不返回 JSON envelope，直接返回文件流或 text/vtt。',
      'GET /stream 支持 Range 请求，拖动进度条时通常返回 206 Partial Content。',
      'audio_track_id 会触发后端验证并生成 remux 缓存变体，这不是多码率转码。',
      '字幕接口会把 srt、ass/ssa、内嵌字幕统一转换成浏览器可挂载的 WebVTT。',
    ],
    endpoints: [
      { method: 'GET', path: '/api/media-files/{id}/audio-tracks', description: '查询媒体文件可切换的内嵌音轨列表' },
      { method: 'GET', path: '/api/media-files/{id}/subtitles', description: '查询媒体文件可切换字幕列表' },
      { method: 'GET', path: '/api/subtitle-files/{id}/stream', description: '输出单条字幕轨道的 WebVTT 内容' },
      { method: 'GET', path: '/api/media-files/{id}/stream', description: '播放媒体文件' },
      { method: 'HEAD', path: '/api/media-files/{id}/stream', description: '查询媒体文件播放头信息' },
    ],
  },
]

export const apiIdRelations = [
  ['library_id', '来自 /api/libraries，用于媒体库相关接口'],
  ['media_item_id', '来自媒体库 media-items，用于详情、文件列表和播放进度'],
  ['media_file_id', '来自 /api/media-items/{id}/files，用于播放媒体流和进度上报'],
  ['audio_track_id', '来自 /api/media-files/{id}/audio-tracks，用于切换内嵌音轨'],
  ['subtitle_file_id', '来自 /api/media-files/{id}/subtitles，用于加载单条字幕轨道'],
]

export const apiPlaybackFlow = [
  'GET /api/libraries/{library_id}/media-items',
  'GET /api/media-items/{media_item_id}/files',
  'GET /api/media-files/{media_file_id}/audio-tracks',
  'GET /api/media-files/{media_file_id}/subtitles',
  'GET /api/subtitle-files/{subtitle_file_id}/stream',
  'GET /api/media-files/{media_file_id}/stream',
  'PUT /api/media-items/{media_item_id}/playback-progress',
]
