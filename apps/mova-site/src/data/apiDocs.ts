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
    value: 'error_code + params',
    text: '业务接口统一 JSON envelope；客户端按 error_code 和 params 本地化错误，媒体流和图片资源直接返回文件流。',
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
  'health、bootstrap-status、bootstrap-admin、login、token-login、refresh 和 logout 可匿名访问，其余接口都要求登录态。',
  '管理类接口允许 owner 和 admin；用户角色提升等所有者操作只允许 owner。',
  'Web 端使用 session cookie；原生客户端使用 access token，refresh token 仅用于调用 refresh 接口。',
  'realtime/events 返回 text/event-stream，不使用统一 JSON envelope；重连后应先请求 realtime/state。',
  '媒体条目图片 URL 会带版本参数，浏览器可长期缓存；元数据更新后版本会变化。',
  'code 始终是数字 HTTP 状态码；错误响应使用稳定的 error_code 和 params，message 只作为诊断兜底。',
  '账户与用户管理使用独立业务错误码；客户端应本地化已知错误码，并只在遇到未知错误码时使用 message 兜底。',
  '密码认证失败达到限制时返回 429 和 Retry-After；Web 与原生客户端对同一账户共享失败计数。',
  'TMDB token 来自 MOVA_TMDB_ACCESS_TOKEN；远端评分目前只主动请求 TMDB，合法 NFO 评分会按其 source 和来源持久化，其他外部 ID 用于跨来源识别。',
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
  ['413', 'Payload Too Large，媒体处理输入或结果超过服务端上限'],
  ['429', 'Too Many Requests，认证尝试过多'],
  ['416', 'Range Not Satisfiable，媒体 Range 越界'],
  ['500', 'Internal Server Error，服务内部错误'],
  ['503', 'Service Unavailable，媒体处理资源或依赖服务暂不可用'],
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
  "error_code": "resource_not_found",
  "params": {},
  "message": "media item not found: 42",
  "data": null
}`

export const apiEndpointGroups: ApiEndpointGroup[] = [
  {
    id: 'health',
    title: '健康检查',
    summary: '用于探测服务进程和数据库是否可用，适合容器探针、本地调试和部署后的联通性检查。',
    highlights: [
      '匿名可访问',
      '返回服务状态、权威构建版本和 HTTP API 契约版本',
      '适合作为部署后第一条检查接口',
    ],
    endpoints: [{ method: 'GET', path: '/api/health', description: '健康检查' }],
  },
  {
    id: 'auth-realtime',
    title: '认证、用户与实时同步',
    summary: '覆盖首次初始化、Cookie / Bearer 登录、Token 轮换、首页快照、资源 revision、SSE 和管理员用户管理。',
    highlights: [
      'bootstrap 只在系统没有管理账户时创建唯一 owner，并直接建立登录态。',
      '账户按去除首尾空白后的小写值唯一；Web Session 与原生 Token 都只在数据库保存 hash。',
      'token-login 返回短期 access token 和长期 refresh token，refresh 会轮换两者。',
      '同一设备必须串行 refresh：原始有效期内重放旧 refresh token 会原子撤销设备会话，已过期的历史 token 不影响当前会话。',
      '密码认证默认在 5 分钟内允许 5 次失败，受限时返回 429 和 Retry-After。',
      '/api/home 返回当前用户的有界首页快照，并携带 realtime revision 基线。',
      'SSE 只承载资源失效与临时进度；断线恢复必须使用 /api/realtime/state。',
      'SSE 与连接凭据有效期绑定；收到 credential_expired 后先更新登录凭据并读取 /api/realtime/state，再重新连接。',
      '登出、改密或 refresh token 重放撤销会话时，仅对应凭据的 SSE 收到 session_revoked 并关闭。',
    ],
    endpoints: [
      { method: 'GET', path: '/api/auth/bootstrap-status', description: '查询是否需要初始化系统所有者' },
      { method: 'POST', path: '/api/auth/bootstrap-admin', description: '初始化系统所有者并登录' },
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
      { method: 'PATCH', path: '/api/users/{id}', description: '更新低权限用户的角色、启用状态与媒体库权限（管理员）' },
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
      '通知和已读状态持久化在 PostgreSQL；SSE 只推进通知 revision，客户端随后重新读取通知接口。',
      '统一通知骨架表达类型、结果、对象、原因和诊断，各业务类型可以追加自己的详情。',
      'scan.completed、scan.completed_with_issues、scan.failed 和 scan.cancelled 是扫描终态的权威结果，客户端不得从级别或计数反推结果。',
      '扫描统计只有在 summary_available 为 true 时才可展示；失败或取消时不得把默认 0 当作真实统计。',
      '扫描通知使用 reason_code 和 reason_params 生成本地化主文案，diagnostic_message 只作为次级排障信息。',
      '已有远端身份在 provider 临时故障时保持 matched，并以 metadata_provider_error 表示刷新失败。',
      'metadata.tmdb.retention_expired 是媒体库 warning，表示条目的 TMDB 元数据超过 180 天仍未重新验证；provider-owned 数据与缓存已清除，payload 只保留本地定位字段，不保留原 TMDB 条目 ID。',
      'GET 支持可选 unread_only 过滤；通知中心标记已读后重新请求未读列表，使已读项立即消失。',
      'GET 响应的未读统计不受 category 筛选影响。',
      '标记已读操作幂等，只有状态首次变化时才推进 revision。',
    ],
    endpoints: [
      { method: 'GET', path: '/api/notifications', description: '查询当前用户可见的通知，可选仅返回未读项' },
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
      '媒体库统一自动识别电影和剧集，无需用户手动选择库类型。',
      'metadata_language 支持 zh-CN / en-US，影响扫描和 TMDB 元数据补全语言。',
      '语言变更、缓存失效、catalog revision 和新扫描任务会在同一事务中提交。',
      '仍有活跃扫描时语言变更返回 409，不会提交半更新状态。',
      '创建媒体库后会自动触发一次后台扫描；媒体库不提供启用/禁用状态。',
      '删除媒体库会由数据库级联清理权威数据，并持久化后台任务删除该库独立的图片、字幕和音轨缓存。',
      '搜索会在当前用户可见库内匹配电影、剧集和本地可用的集条目。',
      '搜索结果会返回条目自身的来源原生 ratings 数组；远端评分来自 TMDB，本地 NFO 评分保留自身 source。',
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
      'metadata_provider_item_id、provider_item_id 和 person_id 都是字符串，客户端不得假设远端 ID 一定是数字。',
      'metadata_status 使用 pending / matched / unmatched / failed / skipped 表达元数据处理状态。',
      '条目详情返回 tagline、premiere_date、content_rating 和 ratings 等轻量字段；评分按 source、audience / critic 类型与实际 retrieved_via 来源区分。',
      'metadata-sources 是管理员诊断接口：集合只返回 external_ids、credits 和不含 payload 的来源摘要，不访问文件系统。',
      '单个 metadata source 详情才返回标准化 payload，并在媒体库根目录边界内观察一个 NFO；以 valid / invalid / missing 和稳定 error code 表达当前状态，不调用 ffprobe 或 TMDB；超过结构化解析上限时整份来源无效且不截断。',
      'NFO 标准 payload 区分正式与自定义分级、作品与元数据语言，并支持旧式 ID、结构化评分、actor IDs / profile、季标题/简介/图片及类型不丢失的 artwork；lockdata 仅回显兼容信息，不建立字段锁。',
      '单条元数据刷新枚举逻辑条目的全部本地载体并统一选择 NFO；series 使用全部本地季集文件定位 tvshow.nfo，只有代表文件执行 ffprobe。',
      '已有 matched TMDB binding 时按该 ID 刷新；无效 NFO 保留 last-known-good，冲突 NFO 不能静默换绑。',
      '剧集可通过 seasons、episodes、episode-outline 获取本地可用集和远端大纲合并结果。',
      'episode-outline 的播放快照包含 last_media_file_id；同一集有多个文件版本时，客户端应恢复最近播放的具体版本。',
      'playback-header 会先返回播放器头部；缺少片头区间时，服务端在后台按需检测，不阻塞首次播放。',
      'poster/backdrop/logo 返回经过媒体库边界、大小和图片内容校验的本地图片流；详情只透出可信的 TMDB 官方远程图片地址。',
    ],
    endpoints: [
      { method: 'GET', path: '/api/media-items/{id}', description: '查询单个媒体条目详情' },
      { method: 'GET', path: '/api/media-items/{id}/metadata-sources', description: '查询条目的元数据来源摘要（管理员）' },
      { method: 'GET', path: '/api/media-items/{id}/metadata-sources/{source_id}', description: '查询并观察单个本地元数据来源（管理员）' },
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
      '进度按用户与媒体条目唯一；多个文件版本共享进度，last_media_file_id 只记录最近选择的版本。',
      '最近选择的文件被删除时，继续观看记录会保留，服务端统一回退到同一条目的首个现存版本。',
      '重复媒体条目合并时，文件、进度和继续观看状态在同一事务迁移，并保留较新的观看状态。',
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
      'GET 携带 audio_track_id 时会验证并按需生成 remux 缓存。',
      '音轨和字幕 HEAD 都是只读探测；缓存命中返回准确头，缓存未命中返回 no-store 且不返回虚假长度，也不启动 FFmpeg。',
      '音轨缓存命中会立即返回；生成槽位已满或同 key 等待超时时返回 503，由客户端稍后重试。',
      '字幕接口会把 srt、ass/ssa、内嵌字幕统一转换成浏览器可挂载的 WebVTT。',
      '字幕源和 WebVTT 结果均有大小上限；超限返回 413 和 subtitle_too_large。',
      '媒体与外部字幕的真实路径必须位于其所属媒体库根目录内。',
    ],
    endpoints: [
      { method: 'GET', path: '/api/media-files/{id}/audio-tracks', description: '查询媒体文件可切换的内嵌音轨列表' },
      { method: 'GET', path: '/api/media-files/{id}/subtitles', description: '查询媒体文件可切换字幕列表' },
      { method: 'GET', path: '/api/subtitle-files/{id}/stream', description: '输出单条字幕轨道的 WebVTT 内容' },
      { method: 'HEAD', path: '/api/subtitle-files/{id}/stream', description: '只读查询字幕 WebVTT 头信息，不生成字幕缓存' },
      { method: 'GET', path: '/api/media-files/{id}/stream', description: '播放媒体文件' },
      { method: 'HEAD', path: '/api/media-files/{id}/stream', description: '只读查询播放头信息，不生成音轨缓存' },
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
