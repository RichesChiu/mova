import type { UserAccount } from '../api/types'

export const canManageLibraries = (viewer: UserAccount) => viewer.role === 'admin'

export const canManageServer = (viewer: UserAccount) => viewer.role === 'admin'

export const canManageAdminAccounts = (viewer: UserAccount) =>
  viewer.role === 'admin' && viewer.is_primary_admin
