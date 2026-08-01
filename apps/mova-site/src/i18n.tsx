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
  '加入 Telegram 群': 'Join the Telegram group',
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
  核心能力: 'Core capabilities',
  支持: 'Support',
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
  代理填写规则: 'Proxy configuration',
  'HTTP_PROXY 和 HTTPS_PROXY 主要用于中国大陆网络访问 TMDB 元数据与图片。代理值必须是容器能够访问的完整 URL，格式为“协议://宿主机实际地址:代理端口”。':
    'HTTP_PROXY and HTTPS_PROXY are primarily intended for reaching TMDB metadata and artwork from mainland China. Each value must be a complete URL reachable from the container, in the format protocol://actual-host-address:proxy-port.',
  '假设宿主机的局域网地址是 192.168.1.1，HTTP 代理端口是 7890，应按下面的方式填写；请根据自己的实际地址和端口替换示例值。':
    'If the Docker host LAN address is 192.168.1.1 and the HTTP proxy port is 7890, use the values below. Replace the example address and port with your own.',
  '代理程序必须允许来自 Docker 网络或局域网的连接。不要使用 127.0.0.1 或 localhost，它们在容器内指向 MOVA 容器自身。':
    'The proxy must accept connections from the Docker network or LAN. Do not use 127.0.0.1 or localhost; inside the container they refer to the MOVA container itself.',
  '如果 MOVA 容器已经能直接访问 TMDB，例如透明代理、TUN 或路由器代理已经对 Docker 生效，则两个变量可以留空。仅在宿主机启动普通代理程序不会自动让容器继承代理。':
    'Leave both variables empty when the MOVA container can already reach TMDB, for example through a transparent proxy, TUN, or router proxy that also covers Docker. Merely starting a regular proxy on the host does not make the container inherit it.',
  '这里的代理只影响 MOVA 运行时请求；Docker Hub 镜像拉取代理仍需在 Docker Desktop 或 Docker Engine 中配置。':
    'These variables affect only MOVA runtime requests. Configure image-pull proxy access separately in Docker Desktop or Docker Engine.',
  '获取 TMDB Access Token': 'Get a TMDB Access Token',
  'TMDB Token 用于获取影片与剧集的元数据、海报、背景图、标题 Logo 和评分。':
    'The TMDB token enables movie and series metadata, posters, backdrops, title logos, and ratings.',
  注册并验证账户: 'Register and verify your account',
  '打开 TMDB，注册或登录账户并完成邮箱验证。':
    'Open TMDB, register or sign in, and complete email verification.',
  '申请 API 访问权限': 'Request API access',
  '建议使用桌面浏览器打开账户设置中的 API 页面，按页面要求提交申请并接受 TMDB 条款。':
    'Use a desktop browser to open the API page in account settings, submit the requested information, and accept the TMDB terms.',
  '打开 TMDB API 设置': 'Open TMDB API settings',
  '复制正确的 Token': 'Copy the correct token',
  '申请通过后，在同一页面复制 API Read Access Token。MOVA 使用的是这段较长的 Bearer Token，不是 API Key (v3 auth)。':
    'After approval, copy the API Read Access Token from the same page. MOVA uses this longer Bearer token, not the API Key (v3 auth).',
  '写入 Compose 并重启': 'Add it to Compose and restart',
  '将 Token 填入 docker-compose.yml 的 MOVA_TMDB_ACCESS_TOKEN，然后执行 docker compose up -d。':
    'Set MOVA_TMDB_ACCESS_TOKEN in docker-compose.yml, then run docker compose up -d.',
  '你的_API_Read_Access_Token': 'YOUR_API_READ_ACCESS_TOKEN',
  'Token 属于敏感凭据，不要提交到 Git 仓库或写入公开日志。':
    'The token is a sensitive credential. Do not commit it to Git or include it in public logs.',
  '不配置 Token 时，MOVA 仍可启动、扫描本地文件并完成入库，但会跳过 TMDB 元数据与图片刮削。后续补上 Token 并重启服务后，重新扫描媒体库即可补齐远端元数据，无需重建数据库。':
    'Without a token, MOVA still starts, scans local files, and stores them, but skips TMDB metadata and artwork. Add the token later, restart the service, and rescan the library to enrich remote metadata without rebuilding the database.',
  '查看 TMDB 官方认证说明': 'Read the official TMDB authentication guide',
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
  'API 参考': 'API Reference',
  权威来源: 'Source of truth',
  通用: 'General',
  播放器流程: 'Player flow',

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
  '业务接口统一 JSON envelope；客户端按 error_code 和 params 本地化错误，媒体流和图片资源直接返回文件流。':
    'Business endpoints use a consistent JSON envelope; clients localize errors with error_code and params, while media and image resources return file streams directly.',
  登录态: 'Authentication',
  'Web 使用 session cookie，原生客户端使用 token-login 返回的 Bearer token。':
    'The Web app uses a session cookie; native clients use the Bearer token returned by token-login.',
  实时事件: 'Realtime events',
  'GET /api/realtime/events 推送资源失效与临时扫描进度。':
    'GET /api/realtime/events pushes resource invalidation and transient scan progress.',
  'health、bootstrap-status、bootstrap-admin、login、token-login、refresh 和 logout 可匿名访问，其余接口都要求登录态。':
    'health, bootstrap-status, bootstrap-admin, login, token-login, refresh, and logout are public; all other endpoints require authentication.',
  '管理类接口允许 owner 和 admin；用户角色提升等所有者操作只允许 owner。':
    'Administrative endpoints allow owner and admin roles; owner-only operations such as role elevation require the owner role.',
  'Web 端使用 session cookie；原生客户端使用 access token，refresh token 仅用于调用 refresh 接口。':
    'The Web app uses a session cookie; native clients use an access token, and the refresh token is only used by the refresh endpoint.',
  'realtime/events 返回 text/event-stream，不使用统一 JSON envelope；重连后应先请求 realtime/state。':
    'realtime/events returns text/event-stream instead of the JSON envelope; request realtime/state first after reconnecting.',
  '媒体条目图片 URL 会带版本参数，浏览器可长期缓存；元数据更新后版本会变化。':
    'Media image URLs include a version parameter for long-lived browser caching; the version changes after metadata updates.',
  'code 始终是数字 HTTP 状态码；错误响应使用稳定的 error_code 和 params，message 只作为诊断兜底。':
    'code is always a numeric HTTP status; error responses use stable error_code and params fields, while message is only a diagnostic fallback.',
  '账户与用户管理使用独立业务错误码；客户端应本地化已知错误码，并只在遇到未知错误码时使用 message 兜底。':
    'Account and user-management endpoints use dedicated business error codes; clients should localize known codes and use message only as a fallback for unknown codes.',
  '密码认证失败达到限制时返回 429 和 Retry-After；Web 与原生客户端对同一账户共享失败计数。':
    'Password authentication returns 429 and Retry-After when the limit is reached; Web and native clients share the failure count for the same account.',
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
  'Payload Too Large，媒体处理输入或结果超过服务端上限':
    'Payload Too Large, media processing input or output exceeds the server limit',
  'Too Many Requests，认证尝试过多': 'Too Many Requests, too many authentication attempts',
  'Range Not Satisfiable，媒体 Range 越界': 'Range Not Satisfiable, media range is out of bounds',
  'Internal Server Error，服务内部错误': 'Internal Server Error',
  'Service Unavailable，媒体处理资源或依赖服务暂不可用':
    'Service Unavailable, media processing capacity or a dependency is temporarily unavailable',

  健康检查: 'Health',
  '用于探测服务进程和数据库是否可用，适合容器探针、本地调试和部署后的联通性检查。':
    'Checks service and database availability for container probes, local debugging, and post-deployment connectivity tests.',
  匿名可访问: 'Publicly accessible',
  '返回服务状态、权威构建版本和 HTTP API 契约版本':
    'Returns service status, the authoritative build version, and the HTTP API contract version',
  适合作为部署后第一条检查接口: 'A good first check after deployment',
  '认证、用户与实时同步': 'Authentication, users, and realtime sync',
  '覆盖首次初始化、Cookie / Bearer 登录、Token 轮换、首页快照、资源 revision、SSE 和管理员用户管理。':
    'Covers first-time setup, Cookie/Bearer login, token rotation, home snapshots, resource revisions, SSE, and administrator user management.',
  'bootstrap 只在系统没有管理账户时创建唯一 owner，并直接建立登录态。':
    'Bootstrap creates the unique owner only when no management account exists and immediately establishes a session.',
  '账户按去除首尾空白后的小写值唯一；Web Session 与原生 Token 都只在数据库保存 hash。':
    'Accounts are unique by their trimmed lowercase value; Web sessions and native tokens are stored only as hashes.',
  'token-login 返回短期 access token 和长期 refresh token，refresh 会轮换两者。':
    'token-login returns a short-lived access token and a long-lived refresh token; refresh rotates both.',
  '同一设备必须串行 refresh：原始有效期内重放旧 refresh token 会原子撤销设备会话，已过期的历史 token 不影响当前会话。':
    'Refresh requests for one device must be serialized: replaying an old refresh token within its original lifetime atomically revokes that device session, while an expired historical token does not affect the current session.',
  '密码认证默认在 5 分钟内允许 5 次失败，受限时返回 429 和 Retry-After。':
    'Password authentication allows five failures within five minutes by default, then returns 429 with Retry-After.',
  '/api/home 返回当前用户的有界首页快照，并携带 realtime revision 基线。':
    '/api/home returns a bounded home snapshot for the current user with the realtime revision baseline.',
  'SSE 只承载资源失效与临时进度；断线恢复必须使用 /api/realtime/state。':
    'SSE carries resource invalidation and transient progress only; reconnect recovery must use /api/realtime/state.',
  'SSE 与连接凭据有效期绑定；收到 credential_expired 后先更新登录凭据并读取 /api/realtime/state，再重新连接。':
    'SSE is bound to the lifetime of the connection credential; after credential_expired, renew the login credential, read /api/realtime/state, and then reconnect.',
  '登出、改密或 refresh token 重放撤销会话时，仅对应凭据的 SSE 收到 session_revoked 并关闭。':
    'When logout, a password change, or refresh-token replay revokes a session, only the SSE connection using that credential receives session_revoked and closes.',
  查询是否需要初始化系统所有者: 'Check whether the system owner must be initialized',
  初始化系统所有者并登录: 'Initialize the system owner and sign in',
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
  '更新低权限用户的角色、启用状态与媒体库权限（管理员）':
    'Update roles, enabled status, and library access for lower-privilege users (admin)',
  '删除用户（管理员）': 'Delete a user (admin)',
  管理员重置指定用户密码: 'Reset a user password (admin)',
  通知中心: 'Notifications',
  '返回当前用户可见的持久化通知、总未读数和分类未读数，并支持单条或批量标记已读。':
    'Returns persistent notifications visible to the current user, total and category unread counts, and supports marking one or many as read.',
  '标准类别包括 scan、system、library 和 account，未知类别也必须保留展示。':
    'Standard categories include scan, system, library, and account; unknown categories must remain visible.',
  '通知和已读状态持久化在 PostgreSQL；SSE 只推进通知 revision，客户端随后重新读取通知接口。':
    'Notifications and read states are persisted in PostgreSQL; SSE only advances the notification revision, after which clients fetch the notification API again.',
  '统一通知骨架表达类型、结果、对象、原因和诊断，各业务类型可以追加自己的详情。':
    'A shared notification structure describes the type, result, target, reason, and diagnostics, while each business type may add its own details.',
  'scan.completed、scan.completed_with_issues、scan.failed 和 scan.cancelled 是扫描终态的权威结果，客户端不得从级别或计数反推结果。':
    'scan.completed, scan.completed_with_issues, scan.failed, and scan.cancelled are the authoritative scan outcomes; clients must not infer the outcome from severity or counts.',
  '扫描统计只有在 summary_available 为 true 时才可展示；失败或取消时不得把默认 0 当作真实统计。':
    'Scan statistics may be displayed only when summary_available is true; failures and cancellations must not present default zero values as real statistics.',
  '扫描通知使用 reason_code 和 reason_params 生成本地化主文案，diagnostic_message 只作为次级排障信息。':
    'Scan notifications use reason_code and reason_params for localized primary copy; diagnostic_message is secondary troubleshooting information.',
  '已有远端身份在 provider 临时故障时保持 matched，并以 metadata_provider_error 表示刷新失败。':
    'Existing remote identities remain matched during transient provider failures, with metadata_provider_error indicating the refresh failure.',
  'metadata.tmdb.retention_expired 是媒体库 warning，表示条目的 TMDB 元数据超过 180 天仍未重新验证；provider-owned 数据与缓存已清除，payload 只保留本地定位字段，不保留原 TMDB 条目 ID。':
    'metadata.tmdb.retention_expired is a library warning indicating that an item’s TMDB metadata remained unverified beyond 180 days; provider-owned data and caches are cleared, and its payload retains only local identifiers rather than the original TMDB item ID.',
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
  '语言变更、缓存失效、catalog revision 和新扫描任务会在同一事务中提交。':
    'Language changes, cache invalidation, catalog revisions, and the new scan job commit in one transaction.',
  '仍有活跃扫描时语言变更返回 409，不会提交半更新状态。':
    'Changing metadata language while a scan remains active returns 409 without committing partial state.',
  '创建媒体库后会自动触发一次后台扫描；媒体库不提供启用/禁用状态。':
    'Creating a library automatically starts a background scan; libraries do not have an enabled/disabled state.',
  '删除媒体库会由数据库级联清理权威数据，并持久化后台任务删除该库独立的图片、字幕和音轨缓存。':
    'Deleting a library cascades its authoritative database data and persists a background job that removes the library-scoped artwork, subtitle, and audio caches.',
  '搜索会在当前用户可见库内匹配电影、剧集和本地可用的集条目。':
    'Search matches movies, series, and locally available episodes in libraries visible to the current user.',
  '搜索结果会返回条目自身的来源原生 ratings 数组，当前评分来源为 TMDB。':
    'Search results include the item’s source-native ratings array; TMDB is the current rating source.',
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
  'metadata_provider_item_id、provider_item_id 和 person_id 都是字符串，客户端不得假设远端 ID 一定是数字。':
    'metadata_provider_item_id, provider_item_id, and person_id are strings; clients must not assume remote IDs are numeric.',
  'metadata_status 使用 matched / unmatched / failed / skipped 表达元数据处理状态。':
    'metadata_status uses matched, unmatched, failed, and skipped to represent metadata processing state.',
  '剧集可通过 seasons、episodes、episode-outline 获取本地可用集和远端大纲合并结果。':
    'Series use seasons, episodes, and episode-outline to merge locally available episodes with remote outlines.',
  'episode-outline 的播放快照包含 last_media_file_id；同一集有多个文件版本时，客户端应恢复最近播放的具体版本。':
    'episode-outline playback snapshots include last_media_file_id so clients can restore the exact last-used file when an episode has multiple variants.',
  'playback-header 会先返回播放器头部；缺少片头区间时，服务端在后台按需检测，不阻塞首次播放。':
    'playback-header returns player header data first; when intro markers are missing, the server detects them on demand in the background without blocking first playback.',
  'poster/backdrop 返回图片流；若详情字段是远程 URL，前端可直接使用远程地址。':
    'poster/backdrop return image streams; when a detail field is a remote URL, clients may use it directly.',
  'poster/backdrop/logo 返回经过媒体库边界、大小和图片内容校验的本地图片流；详情只透出可信的 TMDB 官方远程图片地址。':
    'poster/backdrop/logo return local image streams validated by library boundary, size, and image content; details expose only trusted official TMDB remote artwork URLs.',
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
  '读取媒体条目透明标题 Logo': 'Read a media item transparent title logo',
  读取某一季海报图: 'Read a season poster',
  读取某一季背景图: 'Read a season backdrop',
  播放进度: 'Playback progress',
  '记录当前用户的播放位置和继续观看列表，所有进度都按登录用户隔离。':
    'Stores playback position and continue-watching state for the current user, isolated per account.',
  '查询进度返回 null 是正常语义，表示当前用户尚未观看该内容。':
    'A null progress response is normal and means the current user has not watched the item.',
  '写入进度时同时提交 media_file_id、position_seconds 和 duration_seconds。':
    'Progress updates include media_file_id, position_seconds, and duration_seconds.',
  '进度按用户与媒体条目唯一；多个文件版本共享进度，last_media_file_id 只记录最近选择的版本。':
    'Progress is unique per user and media item; file variants share progress, while last_media_file_id records the latest selected variant.',
  '最近选择的文件被删除时，继续观看记录会保留，服务端统一回退到同一条目的首个现存版本。':
    'When the last selected file is removed, continue watching is preserved and the server falls back to the first remaining variant for that item.',
  '重复媒体条目合并时，文件、进度和继续观看状态在同一事务迁移，并保留较新的观看状态。':
    'When duplicate media items merge, files, progress, and continue-watching state move in one transaction and the newest viewing state is retained.',
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
  'GET 携带 audio_track_id 时会验证并按需生成 remux 缓存。':
    'GET validates audio_track_id and creates a remux cache on demand.',
  '音轨和字幕 HEAD 都是只读探测；缓存命中返回准确头，缓存未命中返回 no-store 且不返回虚假长度，也不启动 FFmpeg。':
    'Audio and subtitle HEAD requests are read-only probes; cache hits return accurate headers, while misses return no-store without a false length or starting FFmpeg.',
  '音轨缓存命中会立即返回；生成槽位已满或同 key 等待超时时返回 503，由客户端稍后重试。':
    'Audio cache hits return immediately; a full generation gate or same-key wait timeout returns 503 so the client can retry later.',
  '字幕源和 WebVTT 结果均有大小上限；超限返回 413 和 subtitle_too_large。':
    'Subtitle sources and WebVTT output are size-limited; oversized content returns 413 with subtitle_too_large.',
  '媒体与外部字幕的真实路径必须位于其所属媒体库根目录内。':
    'Canonical media and external subtitle paths must remain inside their owning library root.',
  '字幕接口会把 srt、ass/ssa、内嵌字幕统一转换成浏览器可挂载的 WebVTT。':
    'Subtitle endpoints convert srt, ass/ssa, and embedded subtitles to browser-ready WebVTT.',
  查询媒体文件可切换的内嵌音轨列表: 'List selectable embedded audio tracks',
  查询媒体文件可切换字幕列表: 'List selectable subtitles',
  '输出单条字幕轨道的 WebVTT 内容': 'Output one subtitle track as WebVTT',
  '只读查询字幕 WebVTT 头信息，不生成字幕缓存':
    'Read subtitle WebVTT headers without generating a subtitle cache',
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
