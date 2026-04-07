import type { UserAccount } from '../api/types'

export const canManageLibraries = (viewer: UserAccount) => viewer.role === 'admin'

export const canManageServer = (viewer: UserAccount) => viewer.role === 'admin'
