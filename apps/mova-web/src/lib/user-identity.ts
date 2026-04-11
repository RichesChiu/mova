import type { UserAccount } from '../api/types'

type UserIdentity = Pick<UserAccount, 'nickname' | 'username'>

export const getUserDisplayName = (user: UserIdentity) => {
  const nickname = user.nickname.trim()

  return nickname.length > 0 ? nickname : user.username
}

export const getUserInitial = (user: UserIdentity) =>
  getUserDisplayName(user).trim().charAt(0).toUpperCase() || 'U'
