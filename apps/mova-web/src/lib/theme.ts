export const THEME_ATTRIBUTE = 'data-theme'

export const THEMES = {
  noir: 'glass-noir',
  frost: 'glass-frost',
} as const

export type ThemeName = (typeof THEMES)[keyof typeof THEMES]

export const DEFAULT_THEME: ThemeName = THEMES.noir

export function applyTheme(theme: ThemeName = DEFAULT_THEME) {
  const root = document.documentElement
  root.setAttribute(THEME_ATTRIBUTE, theme)
  root.style.colorScheme = theme === THEMES.noir ? 'dark' : 'light'
}
