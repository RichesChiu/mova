import type { UserAccount } from '../api/types'

export type UserRolePresentation = {
  label: 'System Administrator' | 'Administrator' | 'Standard User'
  tone: 'system-admin' | 'admin' | 'user'
}

const getUserManagementLevel = (user: Pick<UserAccount, 'is_primary_admin' | 'role'>): number => {
  if (user.is_primary_admin) {
    return 2
  }

  return user.role === 'admin' ? 1 : 0
}

export const canManageUser = (
  actor: Pick<UserAccount, 'id' | 'is_primary_admin' | 'role'>,
  target: Pick<UserAccount, 'id' | 'is_primary_admin' | 'role'>,
): boolean =>
  actor.id !== target.id && getUserManagementLevel(actor) > getUserManagementLevel(target)

export const getUserRolePresentation = (
  user: Pick<UserAccount, 'is_primary_admin' | 'role'>,
): UserRolePresentation => {
  if (user.is_primary_admin) {
    return { label: 'System Administrator', tone: 'system-admin' }
  }

  if (user.role === 'admin') {
    return { label: 'Administrator', tone: 'admin' }
  }

  return { label: 'Standard User', tone: 'user' }
}
