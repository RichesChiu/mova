import { afterEach, describe, expect, it } from 'vitest'
import { localizeApiError } from './api-error'

describe('localizeApiError', () => {
  afterEach(() => {
    document.documentElement.lang = 'zh-CN'
  })

  it('uses the stable error code instead of the diagnostic message', () => {
    document.documentElement.lang = 'zh-CN'

    expect(localizeApiError('resource_not_found', {}, 'media item not found: 42')).toBe(
      '未找到请求的资源。',
    )
  })

  it('interpolates structured params', () => {
    document.documentElement.lang = 'en-US'

    expect(localizeApiError('rate_limited', { retry_after_seconds: 90 }, 'try again later')).toBe(
      'Too many requests. Try again in 90 seconds.',
    )
  })

  it('uses a generic rate-limit message when retry timing is unavailable', () => {
    document.documentElement.lang = 'en-US'

    expect(localizeApiError('rate_limited', {}, 'upstream detail')).toBe(
      'Too many requests. Try again later.',
    )
  })

  it('localizes structured business validation params', () => {
    document.documentElement.lang = 'zh-CN'

    expect(
      localizeApiError(
        'field_too_long',
        { field: 'account', max: 254 },
        'username must be at most 254 characters long',
      ),
    ).toBe('账户不能超过 254 个字符。')
  })

  it('localizes known business errors without exposing diagnostics', () => {
    document.documentElement.lang = 'zh-CN'

    expect(localizeApiError('account_already_exists', {}, 'duplicate key value')).toBe(
      '此账户名称已存在。',
    )
  })

  it('ignores diagnostics for known client-side error codes', () => {
    document.documentElement.lang = 'en-US'

    expect(localizeApiError('network_error', {}, 'Failed to fetch')).toBe(
      'Unable to reach the server. Check your connection and try again.',
    )
  })

  it('keeps the diagnostic message as the unknown-code fallback', () => {
    document.documentElement.lang = 'zh-CN'

    expect(localizeApiError('future_error', {}, 'future diagnostic')).toBe('future diagnostic')
  })
})
