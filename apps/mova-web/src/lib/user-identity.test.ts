import { describe, expect, it } from 'vitest'
import { getUserDisplayName, getUserInitial } from './user-identity'

describe('user identity helpers', () => {
  it('prefers nickname for display text and initials', () => {
    expect(
      getUserDisplayName({
        username: 'viewer01',
        nickname: 'Cinema Fan',
      }),
    ).toBe('Cinema Fan')
    expect(
      getUserInitial({
        username: 'viewer01',
        nickname: 'Cinema Fan',
      }),
    ).toBe('C')
  })

  it('falls back to username when nickname is blank', () => {
    expect(
      getUserDisplayName({
        username: 'viewer01',
        nickname: '   ',
      }),
    ).toBe('viewer01')
    expect(
      getUserInitial({
        username: 'viewer01',
        nickname: '   ',
      }),
    ).toBe('V')
  })
})
