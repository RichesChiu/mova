import { describe, expect, it } from 'vitest'
import { formatMediaCountry } from './media-country'

describe('formatMediaCountry', () => {
  it('returns null for blank values', () => {
    expect(formatMediaCountry('   ')).toBeNull()
  })

  it('keeps country names as-is', () => {
    expect(formatMediaCountry('China · Japan')).toBe('China · Japan')
  })

  it('maps region codes to readable names when possible', () => {
    expect(formatMediaCountry('JP · US')).toBe('Japan · United States')
  })
})
