import { createContext, useContext, useMemo, useState, type ReactNode } from 'react'
import {
  readStoredInterfaceLanguagePreference,
  setInterfaceLanguagePreference,
  type InterfaceLanguage,
} from '../lib/preferences'
import { formatDateTime } from '../lib/format'
import { translateForLanguage, type Translate } from './catalog'

interface I18nContextValue {
  formatDateTime: (value: string | null | undefined) => string
  formatList: (items: string[]) => string
  formatNumber: (value: number) => string
  language: InterfaceLanguage
  l: Translate
  setLanguage: (language: string) => InterfaceLanguage
}

const I18nContext = createContext<I18nContextValue | null>(null)

export const I18nProvider = ({ children }: { children: ReactNode }) => {
  const [language, setLanguageState] = useState<InterfaceLanguage>(() =>
    readStoredInterfaceLanguagePreference(),
  )

  const value = useMemo<I18nContextValue>(() => {
    const l: Translate = (message, params) => translateForLanguage(language, message, params)

    return {
      formatDateTime: (value) => formatDateTime(value, language),
      formatList: (items) => new Intl.ListFormat(language, { style: 'long', type: 'conjunction' }).format(items),
      formatNumber: (value) => new Intl.NumberFormat(language).format(value),
      language,
      l,
      setLanguage: (nextLanguage) => {
        const normalized = setInterfaceLanguagePreference(nextLanguage)
        setLanguageState(normalized)
        return normalized
      },
    }
  }, [language])

  return <I18nContext.Provider value={value}>{children}</I18nContext.Provider>
}

export const useI18n = () => {
  const context = useContext(I18nContext)

  if (!context) {
    throw new Error('useI18n must be used within I18nProvider')
  }

  return context
}

