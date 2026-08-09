import { afterEach, describe, expect, it } from 'vitest'
import { errorCodeForHttpStatus, localizeApiError } from './api-error'

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

  it('localizes subtitle processing limits without exposing diagnostics', () => {
    document.documentElement.lang = 'zh-CN'

    expect(
      localizeApiError(
        'subtitle_too_large',
        { max_bytes: 16 * 1024 * 1024 },
        'subtitle exceeds the processing limit',
      ),
    ).toBe('所选字幕过大，服务器无法处理。')
  })

  it('maps a bare HTTP 413 response to the subtitle size fallback', () => {
    expect(errorCodeForHttpStatus(413)).toBe('subtitle_too_large')
  })

  it('localizes remote-stream failures without exposing upstream diagnostics', () => {
    const messages = [
      [
        'strm_audio_track_selection_unsupported',
        'Embedded audio track selection is not available for remote streams.',
        '远程流不支持选择内嵌音轨。',
      ],
      ['strm_reference_empty', 'The remote stream reference is empty.', '远程流引用内容为空。'],
      [
        'strm_reference_too_large',
        'The remote stream reference is too large.',
        '远程流引用文件过大。',
      ],
      [
        'strm_reference_invalid_utf8',
        'The remote stream reference is not valid UTF-8 text.',
        '远程流引用不是有效的 UTF-8 文本。',
      ],
      [
        'strm_reference_multiple_lines',
        'The remote stream reference must contain exactly one URL.',
        '远程流引用必须且只能包含一个地址。',
      ],
      [
        'strm_reference_invalid_url',
        'The remote stream reference does not contain a valid URL.',
        '远程流引用没有包含有效地址。',
      ],
      [
        'strm_reference_unsupported_scheme',
        'The remote stream reference must use HTTP or HTTPS.',
        '远程流引用必须使用 HTTP 或 HTTPS。',
      ],
      [
        'strm_reference_credentials_not_allowed',
        'Credentials are not allowed inside a remote stream URL.',
        '远程流地址中不能包含账户或密码。',
      ],
      ['strm_reference_invalid', 'The remote stream reference is invalid.', '远程流引用内容无效。'],
      [
        'strm_target_forbidden',
        'The remote stream target is blocked by the server security policy.',
        '远程流目标已被服务器安全策略拦截。',
      ],
      [
        'remote_range_not_supported',
        'The remote source does not support seeking to this position.',
        '远程资源不支持跳转到此播放位置。',
      ],
      [
        'strm_user_stream_limit_exceeded',
        'You have reached the concurrent remote stream limit. Close another stream and try again.',
        '已达到当前账户的远程流并发上限，请关闭其他播放后重试。',
      ],
      [
        'remote_source_unavailable',
        'The remote media source is temporarily unavailable.',
        '远程媒体资源暂时不可用。',
      ],
      [
        'remote_response_invalid',
        'The remote source did not return a supported media response.',
        '远程资源没有返回受支持的媒体响应。',
      ],
      [
        'remote_source_timeout',
        'The remote media source took too long to respond.',
        '远程媒体资源响应超时。',
      ],
      [
        'strm_stream_capacity_exhausted',
        'The server is currently handling too many remote streams. Try again shortly.',
        '服务器当前处理的远程流过多，请稍后重试。',
      ],
    ] as const

    document.documentElement.lang = 'zh-CN'
    for (const [errorCode, _english, chinese] of messages) {
      expect(
        localizeApiError(
          errorCode,
          {},
          'request failed for https://media.example/private?token=secret',
        ),
      ).toBe(chinese)
    }

    document.documentElement.lang = 'en-US'
    for (const [errorCode, english] of messages) {
      expect(
        localizeApiError(
          errorCode,
          {},
          'request failed for https://media.example/private?token=secret',
        ),
      ).toBe(english)
    }
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
