import { describe, expect, it } from 'vitest'
import type { UserAccount } from '../api/types'
import { canManageServer } from './viewer'

const buildUser = (overrides: Partial<Pick<UserAccount, 'role'>>): UserAccount => ({
  id: 1,
  username: 'account',
  nickname: 'Account',
  role: 'viewer',
  is_enabled: true,
  library_ids: [],
  created_at: '2026-07-17T00:00:00Z',
  updated_at: '2026-07-17T00:00:00Z',
  ...overrides,
})

describe('canManageServer', () => {
  it('allows the owner and administrators', () => {
    expect(canManageServer(buildUser({ role: 'owner' }))).toBe(true)
    expect(canManageServer(buildUser({ role: 'admin' }))).toBe(true)
  })

  it('keeps server settings hidden from standard users', () => {
    expect(canManageServer(buildUser({ role: 'viewer' }))).toBe(false)
  })
})
