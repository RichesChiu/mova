import { createContext, useContext } from 'react'

export type Language = 'zh' | 'en'

export type I18nContextValue = {
  language: Language
  setLanguage: (language: Language) => void
  t: (text: string) => string
}

export const I18nContext = createContext<I18nContextValue | null>(null)

export function useI18n() {
  const context = useContext(I18nContext)

  if (!context) {
    throw new Error('useI18n must be used within I18nProvider')
  }

  return context
}
