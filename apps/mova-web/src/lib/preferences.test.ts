import { beforeEach, describe, expect, it } from 'vitest'
import { THEME_ATTRIBUTE, THEMES } from './theme'
import {
  DEFAULT_INTERFACE_LANGUAGE,
  INTERFACE_LANGUAGES,
  initializeAppPreferences,
  normalizeInterfaceLanguagePreference,
  normalizeThemePreference,
  readStoredInterfaceLanguagePreference,
  readStoredThemePreference,
  setInterfaceLanguagePreference,
  setThemePreference,
} from './preferences'

describe('preferences helpers', () => {
  beforeEach(() => {
    window.localStorage.clear()
    document.documentElement.removeAttribute(THEME_ATTRIBUTE)
    document.documentElement.removeAttribute('lang')
    document.documentElement.style.colorScheme = ''
  })

  it('normalizes unsupported preference values back to defaults', () => {
    expect(normalizeThemePreference('unknown')).toBe(THEMES.noir)
    expect(normalizeInterfaceLanguagePreference('ja-JP')).toBe(DEFAULT_INTERFACE_LANGUAGE)
  })

  it('persists and applies the selected theme', () => {
    expect(setThemePreference(THEMES.frost)).toBe(THEMES.frost)
    expect(readStoredThemePreference()).toBe(THEMES.frost)
    expect(document.documentElement.getAttribute(THEME_ATTRIBUTE)).toBe(THEMES.frost)
    expect(document.documentElement.style.colorScheme).toBe('light')
  })

  it('persists and applies the selected interface language', () => {
    expect(setInterfaceLanguagePreference(INTERFACE_LANGUAGES.chinese)).toBe(
      INTERFACE_LANGUAGES.chinese,
    )
    expect(readStoredInterfaceLanguagePreference()).toBe(INTERFACE_LANGUAGES.chinese)
    expect(document.documentElement.lang).toBe(INTERFACE_LANGUAGES.chinese)
  })

  it('bootstraps both preferences from storage on app start', () => {
    window.localStorage.setItem('mova.theme', THEMES.frost)
    window.localStorage.setItem('mova.interfaceLanguage', INTERFACE_LANGUAGES.chinese)

    initializeAppPreferences()

    expect(document.documentElement.getAttribute(THEME_ATTRIBUTE)).toBe(THEMES.frost)
    expect(document.documentElement.style.colorScheme).toBe('light')
    expect(document.documentElement.lang).toBe(INTERFACE_LANGUAGES.chinese)
  })
})
