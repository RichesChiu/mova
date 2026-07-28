import type { UserAccount } from '../api/types'

export type UserRolePresentation = {
  label: 'System Administrator' | 'Administrator' | 'Standard User'
  tone: 'system-admin' | 'admin' | 'user'
}

const getUserManagementLevel = (user: Pick<UserAccount, 'role'>): number =>
  user.role === 'owner' ? 2 : user.role === 'admin' ? 1 : 0

export const canManageUser = (
  actor: Pick<UserAccount, 'id' | 'role'>,
  target: Pick<UserAccount, 'id' | 'role'>,
): boolean =>
  actor.id !== target.id && getUserManagementLevel(actor) > getUserManagementLevel(target)

export const getUserRolePresentation = (user: Pick<UserAccount, 'role'>): UserRolePresentation => {
  if (user.role === 'owner') {
    return { label: 'System Administrator', tone: 'system-admin' }
  }

  if (user.role === 'admin') {
    return { label: 'Administrator', tone: 'admin' }
  }

  return { label: 'Standard User', tone: 'user' }
}
