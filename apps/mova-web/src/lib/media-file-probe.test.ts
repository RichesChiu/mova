import { afterEach, describe, expect, it } from 'vitest'
import { describeMediaFileProbeIssue } from './media-file-probe'

describe('describeMediaFileProbeIssue', () => {
  afterEach(() => {
    document.documentElement.lang = 'zh-CN'
  })

  it('names the category in the interface language', () => {
    document.documentElement.lang = 'en-US'
    expect(describeMediaFileProbeIssue({ probe_issue: 'no_video_stream' })).toBe(
      'The file contains no video stream. It may not play as a video.',
    )

    document.documentElement.lang = 'zh-CN'
    expect(describeMediaFileProbeIssue({ probe_issue: 'unrecognized' })).toBe(
      '文件已损坏、不完整或格式不受支持，未能分析，可能无法播放。',
    )
  })

  it('is silent for an analyzed file', () => {
    expect(describeMediaFileProbeIssue({ probe_issue: null })).toBeNull()
  })
})
