const COUNTRY_CODE_PATTERN = /^[A-Z]{2}$/
const COUNTRY_SEPARATOR_PATTERN = /\s*[·,]\s*/

const regionDisplayNames =
  typeof Intl !== 'undefined' && typeof Intl.DisplayNames === 'function'
    ? new Intl.DisplayNames(['en'], { type: 'region' })
    : null

const normalizeCountryPart = (value: string) => value.trim().toUpperCase()

const formatCountryPart = (value: string) => {
  const trimmedValue = value.trim()

  if (!COUNTRY_CODE_PATTERN.test(trimmedValue)) {
    return trimmedValue
  }

  return regionDisplayNames?.of(normalizeCountryPart(trimmedValue)) ?? trimmedValue
}

export const formatMediaCountry = (value: string | null | undefined) => {
  const normalizedValue = value?.trim()

  if (!normalizedValue) {
    return null
  }

  const parts = normalizedValue
    .split(COUNTRY_SEPARATOR_PATTERN)
    .map((part) => part.trim())
    .filter(Boolean)

  if (parts.length === 0) {
    return null
  }

  return parts.map(formatCountryPart).join(' · ')
}
