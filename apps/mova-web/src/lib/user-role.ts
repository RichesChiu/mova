import type { UserAccount } from '../api/types'

export type UserRolePresentation = {
  label: 'System Administrator' | 'Administrator' | 'Standard User'
  tone: 'system-admin' | 'admin' | 'user'
}

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
