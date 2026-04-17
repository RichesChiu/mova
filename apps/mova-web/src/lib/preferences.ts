import { DEFAULT_THEME, THEMES, applyTheme, type ThemeName } from './theme'

export const INTERFACE_LANGUAGES = {
  english: 'en-US',
  chinese: 'zh-CN',
} as const

export type InterfaceLanguage =
  (typeof INTERFACE_LANGUAGES)[keyof typeof INTERFACE_LANGUAGES]

export const DEFAULT_INTERFACE_LANGUAGE: InterfaceLanguage = INTERFACE_LANGUAGES.english

const STORAGE_KEYS = {
  interfaceLanguage: 'mova.interfaceLanguage',
  theme: 'mova.theme',
} as const

const canUseBrowserPreferences = () =>
  typeof window !== 'undefined' && typeof document !== 'undefined'

const readPreference = (key: string) => {
  if (!canUseBrowserPreferences()) {
    return null
  }

  try {
    return window.localStorage.getItem(key)
  } catch {
    return null
  }
}

const writePreference = (key: string, value: string) => {
  if (!canUseBrowserPreferences()) {
    return
  }

  try {
    window.localStorage.setItem(key, value)
  } catch {
    // Ignore storage write failures so the UI still updates for the current session.
  }
}

export const normalizeThemePreference = (value: string | null | undefined): ThemeName =>
  value === THEMES.frost ? THEMES.frost : DEFAULT_THEME

export const normalizeInterfaceLanguagePreference = (
  value: string | null | undefined,
): InterfaceLanguage =>
  value === INTERFACE_LANGUAGES.chinese ? INTERFACE_LANGUAGES.chinese : DEFAULT_INTERFACE_LANGUAGE

export const readStoredThemePreference = () =>
  normalizeThemePreference(readPreference(STORAGE_KEYS.theme))

export const readStoredInterfaceLanguagePreference = () =>
  normalizeInterfaceLanguagePreference(readPreference(STORAGE_KEYS.interfaceLanguage))

export const applyInterfaceLanguage = (
  language: InterfaceLanguage = DEFAULT_INTERFACE_LANGUAGE,
) => {
  if (!canUseBrowserPreferences()) {
    return
  }

  document.documentElement.lang = language
}

export const setThemePreference = (theme: string) => {
  const normalizedTheme = normalizeThemePreference(theme)
  applyTheme(normalizedTheme)
  writePreference(STORAGE_KEYS.theme, normalizedTheme)
  return normalizedTheme
}

export const setInterfaceLanguagePreference = (language: string) => {
  const normalizedLanguage = normalizeInterfaceLanguagePreference(language)
  applyInterfaceLanguage(normalizedLanguage)
  writePreference(STORAGE_KEYS.interfaceLanguage, normalizedLanguage)
  return normalizedLanguage
}

export const initializeAppPreferences = () => {
  const theme = readStoredThemePreference()
  const interfaceLanguage = readStoredInterfaceLanguagePreference()

  applyTheme(theme)
  applyInterfaceLanguage(interfaceLanguage)
}
