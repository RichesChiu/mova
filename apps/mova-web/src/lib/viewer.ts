import type { UserAccount } from '../api/types'

export function canManageLibraries(viewer: UserAccount) {
  return viewer.role === 'admin'
}

export function canManageServer(viewer: UserAccount) {
  return viewer.role === 'admin'
}
