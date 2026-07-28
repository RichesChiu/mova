import { describe, expect, it } from 'vitest'
import { canManageUser, getUserRolePresentation } from './user-role'

describe('getUserRolePresentation', () => {
  it('presents the owner account as the system administrator', () => {
    expect(getUserRolePresentation({ role: 'owner' })).toEqual({
      label: 'System Administrator',
      tone: 'system-admin',
    })
  })

  it('presents other admin accounts as administrators', () => {
    expect(getUserRolePresentation({ role: 'admin' })).toEqual({
      label: 'Administrator',
      tone: 'admin',
    })
  })

  it('presents viewer accounts as standard users', () => {
    expect(getUserRolePresentation({ role: 'viewer' })).toEqual({
      label: 'Standard User',
      tone: 'user',
    })
  })
})

describe('canManageUser', () => {
  const owner = { id: 1, role: 'owner' as const }
  const admin = { id: 2, role: 'admin' as const }
  const peerAdmin = { id: 3, role: 'admin' as const }
  const viewer = { id: 4, role: 'viewer' as const }

  it('allows only strictly higher privilege levels to manage a user', () => {
    expect(canManageUser(owner, admin)).toBe(true)
    expect(canManageUser(owner, viewer)).toBe(true)
    expect(canManageUser(admin, viewer)).toBe(true)
    expect(canManageUser(admin, peerAdmin)).toBe(false)
    expect(canManageUser(viewer, admin)).toBe(false)
  })

  it('never allows a user to manage themselves', () => {
    expect(canManageUser(owner, owner)).toBe(false)
    expect(canManageUser(admin, admin)).toBe(false)
  })
})
