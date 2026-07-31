import { type TranslationParams, translateCurrent } from '../i18n'

export type ApiErrorParams = Record<string, unknown>

const errorMessageKeys: Record<string, string> = {
  bootstrap_unavailable: 'Initial setup has already been completed.',
  authentication_required: 'Sign in to continue.',
  account_disabled: 'This account is disabled.',
  account_already_exists: 'An account with this name already exists.',
  user_not_found: 'The requested user was not found.',
  self_management_not_allowed: 'You cannot perform this action on your own account here.',
  password_unchanged: 'The new password must be different from the current password.',
  invalid_current_password: 'The current password is incorrect.',
  invalid_credentials: 'The account or password is incorrect.',
  invalid_role: 'Select a valid user role.',
  insufficient_privilege: 'You cannot manage an account with the same or a higher role.',
  admin_required: 'Administrator permission is required.',
  owner_required: 'System administrator permission is required.',
  owner_role_not_assignable: 'The system administrator role cannot be assigned.',
  last_admin_required: 'At least one enabled administrator account is required.',
  invalid_request: 'The request is invalid.',
  resource_conflict: 'The request conflicts with the current resource state.',
  unauthorized: 'Authentication failed.',
  token_expired: 'Your session has expired. Sign in again.',
  invalid_token: 'Your session is invalid. Sign in again.',
  invalid_refresh_token: 'Your refresh token is invalid. Sign in again.',
  refresh_token_expired: 'Your refresh token has expired. Sign in again.',
  session_revoked: 'This session has been revoked. Sign in again.',
  forbidden: 'You do not have permission to perform this action.',
  resource_not_found: 'The requested resource was not found.',
  rate_limited: 'Too many requests. Try again later.',
  service_unavailable: 'The service is temporarily unavailable.',
  range_not_satisfiable: 'The requested media range is not available.',
  internal_error: 'The server encountered an unexpected error.',
  network_error: 'Unable to reach the server. Check your connection and try again.',
  invalid_response: 'The server returned an invalid response.',
  scan_execution_failed: 'The library scan could not be completed.',
  metadata_provider_error: 'Metadata provider request failed',
  no_remote_match: 'No exact metadata match',
  metadata_provider_disabled: 'Metadata provider is disabled',
  metadata_processing_failed: 'Metadata processing failed',
  media_probe_warning: 'Media inspection warning',
  subtitle_too_large: 'The selected subtitle is too large to process.',
  cache_cleanup_failed:
    'The library data was deleted, but its cache could not be removed after all retries.',
  tmdb_retention_expired:
    'TMDB metadata could not be revalidated within 180 days. Provider-owned metadata and cached data were cleared, and the item is ready to be matched again.',
}

const fieldMessageKeys: Record<string, string> = {
  account: 'Account',
  nickname: 'Nickname',
  password: 'Password',
  current_password: 'Password',
  new_password: 'Password',
}

const toTranslationParams = (params: ApiErrorParams): TranslationParams =>
  Object.fromEntries(
    Object.entries(params).flatMap(([key, value]) =>
      typeof value === 'string' || typeof value === 'number' || value === null
        ? [[key, value]]
        : [],
    ),
  )

export const errorCodeForHttpStatus = (status: number): string => {
  switch (status) {
    case 400:
      return 'invalid_request'
    case 401:
      return 'unauthorized'
    case 403:
      return 'forbidden'
    case 404:
      return 'resource_not_found'
    case 409:
      return 'resource_conflict'
    case 413:
      return 'subtitle_too_large'
    case 416:
      return 'range_not_satisfiable'
    case 429:
      return 'rate_limited'
    case 503:
      return 'service_unavailable'
    default:
      return status >= 500 ? 'internal_error' : 'invalid_request'
  }
}

export const localizeApiError = (
  errorCode: string,
  params: ApiErrorParams = {},
  diagnosticMessage?: string,
): string => {
  if (
    (errorCode === 'field_required' ||
      errorCode === 'field_too_long' ||
      errorCode === 'field_too_short') &&
    typeof params.field === 'string'
  ) {
    const field = translateCurrent(fieldMessageKeys[params.field] ?? 'Field')
    const localizedParams = toTranslationParams({ ...params, field })

    if (errorCode === 'field_required') {
      return translateCurrent('{{field}} is required.', localizedParams)
    }
    if (errorCode === 'field_too_long' && typeof params.max === 'number') {
      return translateCurrent('{{field}} must be at most {{max}} characters.', localizedParams)
    }
    if (errorCode === 'field_too_short' && typeof params.min === 'number') {
      return translateCurrent('{{field}} must be at least {{min}} characters.', localizedParams)
    }
  }

  if (
    errorCode === 'rate_limited' &&
    typeof params.retry_after_seconds === 'number' &&
    Number.isFinite(params.retry_after_seconds)
  ) {
    return translateCurrent(
      'Too many requests. Try again in {{retry_after_seconds}} seconds.',
      toTranslationParams(params),
    )
  }

  const messageKey = errorMessageKeys[errorCode]
  if (!messageKey) {
    return diagnosticMessage?.trim() || translateCurrent('The request could not be completed.')
  }

  return translateCurrent(messageKey, toTranslationParams(params))
}
