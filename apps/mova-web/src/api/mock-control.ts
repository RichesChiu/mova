const MOCK_STORAGE_KEY = 'mova:mock-api'
const MOCK_QUERY_KEY = 'mova_mock_api'

const enabledValues = new Set(['1', 'true', 'on', 'yes'])
const disabledValues = new Set(['0', 'false', 'off', 'no'])

const normalizeToggleValue = (value: string | null | undefined) => {
  const normalizedValue = value?.trim().toLowerCase()
  if (!normalizedValue) {
    return null
  }
  if (enabledValues.has(normalizedValue)) {
    return true
  }
  if (disabledValues.has(normalizedValue)) {
    return false
  }
  return null
}

const browserWindow = () => (typeof window === 'undefined' ? null : window)

export const isMockApiBuildEnabled = () => import.meta.env.DEV

export const isMockApiEnabled = () => {
  if (!isMockApiBuildEnabled()) {
    return false
  }

  const currentWindow = browserWindow()
  if (currentWindow) {
    const queryValue = normalizeToggleValue(
      new URLSearchParams(currentWindow.location.search).get(MOCK_QUERY_KEY),
    )
    if (queryValue !== null) {
      currentWindow.localStorage.setItem(MOCK_STORAGE_KEY, queryValue ? '1' : '0')
      return queryValue
    }

    const storedValue = normalizeToggleValue(currentWindow.localStorage.getItem(MOCK_STORAGE_KEY))
    if (storedValue !== null) {
      return storedValue
    }
  }

  return normalizeToggleValue(import.meta.env.VITE_MOVA_MOCK_API) ?? false
}
