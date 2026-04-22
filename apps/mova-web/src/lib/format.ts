import { readStoredInterfaceLanguagePreference } from './preferences'

const resolveLocale = (locale?: string) => locale ?? readStoredInterfaceLanguagePreference()

export const formatDateTime = (value: string | null | undefined, locale?: string) => {
  if (!value) {
    return '—'
  }

  return new Intl.DateTimeFormat(resolveLocale(locale), {
    dateStyle: 'medium',
    timeStyle: 'short',
  }).format(new Date(value))
}

export const formatBytes = (value: number | null | undefined, locale?: string) => {
  if (typeof value !== 'number' || !Number.isFinite(value)) {
    return '—'
  }

  const units = ['B', 'KB', 'MB', 'GB', 'TB']
  let size = value
  let unitIndex = 0

  while (size >= 1024 && unitIndex < units.length - 1) {
    size /= 1024
    unitIndex += 1
  }

  const formatted = new Intl.NumberFormat(resolveLocale(locale), {
    maximumFractionDigits: size >= 10 || unitIndex === 0 ? 0 : 1,
    minimumFractionDigits: size >= 10 || unitIndex === 0 ? 0 : 1,
  }).format(size)

  return `${formatted} ${units[unitIndex]}`
}

export const formatDuration = (seconds: number | null | undefined, locale?: string) => {
  if (typeof seconds !== 'number' || !Number.isFinite(seconds) || seconds <= 0) {
    return '—'
  }

  const resolvedLocale = resolveLocale(locale)

  const hours = Math.floor(seconds / 3600)
  const minutes = Math.floor((seconds % 3600) / 60)
  const remainingSeconds = seconds % 60

  if (resolvedLocale === 'zh-CN') {
    if (hours > 0) {
      return `${hours}小时 ${minutes}分钟 ${remainingSeconds}秒`
    }

    return `${minutes}分钟 ${remainingSeconds}秒`
  }

  if (hours > 0) {
    return `${hours}h ${minutes}m ${remainingSeconds}s`
  }

  return `${minutes}m ${remainingSeconds}s`
}
