import { describe, expect, it } from 'vitest'
import { canManageUser, getUserRolePresentation } from './user-role'

describe('getUserRolePresentation', () => {
  it('presents the primary account as the system administrator', () => {
    expect(getUserRolePresentation({ is_primary_admin: true, role: 'admin' })).toEqual({
      label: 'System Administrator',
      tone: 'system-admin',
    })
  })

  it('presents other admin accounts as administrators', () => {
    expect(getUserRolePresentation({ is_primary_admin: false, role: 'admin' })).toEqual({
      label: 'Administrator',
      tone: 'admin',
    })
  })

  it('presents viewer accounts as standard users', () => {
    expect(getUserRolePresentation({ is_primary_admin: false, role: 'viewer' })).toEqual({
      label: 'Standard User',
      tone: 'user',
    })
  })
})

describe('canManageUser', () => {
  const primaryAdmin = { id: 1, is_primary_admin: true, role: 'admin' as const }
  const admin = { id: 2, is_primary_admin: false, role: 'admin' as const }
  const peerAdmin = { id: 3, is_primary_admin: false, role: 'admin' as const }
  const viewer = { id: 4, is_primary_admin: false, role: 'viewer' as const }

  it('allows only strictly higher privilege levels to manage a user', () => {
    expect(canManageUser(primaryAdmin, admin)).toBe(true)
    expect(canManageUser(primaryAdmin, viewer)).toBe(true)
    expect(canManageUser(admin, viewer)).toBe(true)
    expect(canManageUser(admin, peerAdmin)).toBe(false)
    expect(canManageUser(viewer, admin)).toBe(false)
  })

  it('never allows a user to manage themselves', () => {
    expect(canManageUser(primaryAdmin, primaryAdmin)).toBe(false)
    expect(canManageUser(admin, admin)).toBe(false)
  })
})
