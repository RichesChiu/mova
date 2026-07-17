import { describe, expect, it } from 'vitest'
import type { UserAccount } from '../api/types'
import { canManageServer } from './viewer'

const buildUser = (
  overrides: Partial<Pick<UserAccount, 'is_primary_admin' | 'role'>>,
): UserAccount => ({
  id: 1,
  username: 'account',
  nickname: 'Account',
  role: 'viewer',
  is_primary_admin: false,
  is_enabled: true,
  library_ids: [],
  created_at: '2026-07-17T00:00:00Z',
  updated_at: '2026-07-17T00:00:00Z',
  ...overrides,
})

describe('canManageServer', () => {
  it('allows both the initial system administrator and other administrators', () => {
    expect(canManageServer(buildUser({ role: 'admin', is_primary_admin: true }))).toBe(true)
    expect(canManageServer(buildUser({ role: 'admin', is_primary_admin: false }))).toBe(true)
  })

  it('keeps server settings hidden from standard users', () => {
    expect(canManageServer(buildUser({ role: 'viewer', is_primary_admin: false }))).toBe(false)
  })
})
