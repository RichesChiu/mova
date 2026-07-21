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
  id: 'web' | 'macos' | 'phone' | 'pad'
  title: string
  text: string
  available: boolean
  action?: {
    href: string
    label: string
    variant: 'primary' | 'secondary'
  }
}

export const navItems: NavItem[] = [
  { id: 'home', label: '首页' },
  { id: 'deploy', label: '部署' },
  { id: 'api', label: 'API 文档' },
]

export const githubUrl = 'https://github.com/RichesChiu/mova'
export const dockerUrl = 'https://hub.docker.com/repository/docker/richeschiu/mova/general'
export const macAppStoreUrl =
  'macappstore://search.itunes.apple.com/WebObjects/MZSearch.woa/wa/search?term=MOVA'

export const heroBadges = [
  { icon: 'data-shield', label: '隐私优先', text: '媒体和账户数据始终由你掌控。' },
  { icon: 'rocket', label: '开源可信', text: '完整开源透明，安全可审阅。' },
  { icon: 'multi-terminal', label: '跨设备访问', text: '网页与 macOS 随时访问媒体库。' },
  { icon: 'scalable', label: '持续进化', text: '社区与作者持续完善产品体验。' },
] satisfies { icon: IconName; label: string; text: string }[]

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
    id: 'web',
    title: '网页端',
    text: '在浏览器中随时访问，无需安装。',
    available: true,
    action: {
      href: '#home',
      label: '现在即可使用',
      variant: 'primary',
    },
  },
  {
    id: 'macos',
    title: 'macOS 端',
    text: '原生 macOS 客户端，更优雅的体验。',
    available: true,
    action: {
      href: macAppStoreUrl,
      label: '前往 Mac App Store 安装',
      variant: 'secondary',
    },
  },
  {
    id: 'phone',
    title: '手机端',
    text: '积极开发中，敬请期待。',
    available: false,
  },
  {
    id: 'pad',
    title: 'Pad 端',
    text: '积极开发中，敬请期待。',
    available: false,
  },
]
