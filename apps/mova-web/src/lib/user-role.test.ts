import { describe, expect, it } from 'vitest'
import { getUserRolePresentation } from './user-role'

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
