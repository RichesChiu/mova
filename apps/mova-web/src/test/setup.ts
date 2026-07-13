import '@testing-library/jest-dom/vitest'
import { cleanup } from '@testing-library/react'
import { afterEach, beforeEach } from 'vitest'

beforeEach(() => {
  // Most copy-focused tests assert the English source catalog explicitly. Product-default behavior
  // remains covered independently by preferences.test.ts, which clears this attribute per test.
  document.documentElement.lang = 'en-US'
})

afterEach(() => {
  cleanup()
})
