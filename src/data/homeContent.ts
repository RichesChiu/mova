import type { IconName } from '../components/MovaIcon'

export type NavItem = {
  id: string
  label: string
}

type Feature = {
  icon: IconName
  title: string
  text: string
}

type Device = {
  icon: IconName
  title: string
  text: string
  image: string
}

type Stat = {
  icon: IconName
  value: string
  title: string
  text: string
}

export const navItems: NavItem[] = [
  { id: 'home', label: '首页' },
  { id: 'deploy', label: '部署' },
  { id: 'api', label: 'API 文档' },
]

export const githubUrl = 'https://github.com/RichesChiu/mova'
export const dockerUrl = 'https://hub.docker.com/repository/docker/richeschiu/mova/general'

export const mediaCards = [
  { title: '奥本海默', meta: '1:32:40 / 3:00:00', tone: 'ember' },
  { title: '沙丘 2', meta: '1:15:20 / 2:46:00', tone: 'sand' },
  { title: '星际穿越', meta: '0:48:10 / 2:49:00', tone: 'ice' },
  { title: '怪奇物语', meta: '第 5 季 / 第 9 集', tone: 'neon' },
]

export const heroBadges = [
  { icon: 'rocket', label: '开箱免费' },
  { icon: 'data-shield', label: '隐私安全' },
  { icon: 'multi-terminal', label: '跨平台' },
  { icon: 'scalable', label: '高度可扩展' },
] satisfies { icon: IconName; label: string }[]

export const features: Feature[] = [
  {
    icon: 'private-library',
    title: '私有媒体库',
    text: '集中管理电影、剧集、音乐和照片，本地文件只读挂载，不改动原始媒体。',
  },
  {
    icon: 'device-access',
    title: '多设备访问',
    text: '手机、平板、电脑和电视都能通过 Web 访问，随时随地继续观看。',
  },
  {
    icon: 'transcode',
    title: '高性能转码',
    text: '配合 ffprobe 识别 4K、HDR、Dolby Vision、Atmos 等资源级标签。',
  },
  {
    icon: 'permissions',
    title: '用户与权限管理',
    text: '首次启动创建管理员，后续可按家庭和设备场景管理访问边界。',
  },
  {
    icon: 'metadata',
    title: '元数据整理',
    text: '按文件名归组电影与剧集，可接入 TMDB、OMDB 补齐海报、评分和背景图。',
  },
  {
    icon: 'self-host',
    title: 'Docker 自托管部署',
    text: '一键拉取发布镜像，在服务器或 NAS 上快速运行属于自己的媒体中心。',
  },
]

export const devices: Device[] = [
  {
    icon: 'tv',
    title: '智能电视',
    text: '大屏观影，沉浸体验',
    image: '/screenshots/mova-home.png',
  },
  {
    icon: 'mobile',
    title: '手机',
    text: '随时随地，想看就看',
    image: '/screenshots/mova-theme.png',
  },
  {
    icon: 'tablet',
    title: '平板',
    text: '完美适配，舒适观看',
    image: '/screenshots/mova-home.png',
  },
  {
    icon: 'desktop',
    title: '电脑',
    text: '高效管理，尽在掌握',
    image: '/screenshots/mova-server-setting.png',
  },
]

export const stats: Stat[] = [
  { icon: 'rocket', value: '10 分钟', title: '快速部署', text: '一键安装，极速上线' },
  { icon: 'multi-terminal', value: '多终端支持', title: '全平台覆盖', text: '无需同步，体验一致' },
  { icon: 'data-shield', value: '自主管理数据', title: '隐私安全可控', text: '媒体文件只读挂载' },
  { icon: 'scalable', value: '高可扩展', title: '灵活扩展', text: '插件升级，功能无限' },
]

export const docs = [
  {
    icon: 'docs',
    title: 'API 文档',
    text: '查看 HTTP 接口、鉴权方式、响应格式、媒体流与播放进度说明。',
  },
  {
    icon: 'desktop',
    title: '前端指南',
    text: '整理 Web 端页面、播放器、状态同步和客户端交互边界。',
  },
  {
    icon: 'self-host',
    title: '后端说明',
    text: '了解服务端模块、部署变量、扫描流程和媒体元数据处理逻辑。',
  },
  {
    icon: 'settings',
    title: 'Crates 模块',
    text: '跟踪 Rust crate 分层、核心类型和后续扩展路径。',
  },
] satisfies { icon: IconName; title: string; text: string }[]

export const deploySteps = [
  { icon: 'settings', text: '配置 MOVA_MEDIA_ROOT' },
  { icon: 'self-host', text: 'docker compose up -d' },
  { icon: 'user', text: '创建管理员' },
  { icon: 'library', text: '创建媒体库并扫描' },
] satisfies { icon: IconName; text: string }[]

export const dashboardPrimaryMenu = [
  { icon: 'home', label: '概览' },
  { icon: 'library', label: '媒体库' },
  { icon: 'movie', label: '电影' },
  { icon: 'series', label: '剧集' },
  { icon: 'music', label: '音乐' },
  { icon: 'photo', label: '照片' },
  { icon: 'playlist', label: '播放列表' },
] satisfies { icon: IconName; label: string }[]

export const dashboardAdminMenu = [
  { icon: 'user', label: '用户管理' },
  { icon: 'transcode', label: '转码任务' },
  { icon: 'settings', label: '系统设置' },
] satisfies { icon: IconName; label: string }[]

export const dashboardMetrics = [
  { icon: 'movie', label: '电影', value: '1,248' },
  { icon: 'series', label: '剧集', value: '326' },
  { icon: 'music', label: '音乐', value: '2,156' },
  { icon: 'photo', label: '照片', value: '4,892' },
] satisfies { icon: IconName; label: string; value: string }[]
