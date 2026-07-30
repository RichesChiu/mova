import { describe, expect, it } from 'vitest'
import { isScanSummaryAvailable, resolveNotificationResult } from './notification-presentation'

describe('notification presentation', () => {
  it('uses the scan notification type as the authoritative result', () => {
    expect(resolveNotificationResult('scan.completed', 'error')).toBe('completed')
    expect(resolveNotificationResult('scan.completed_with_issues', 'success')).toBe(
      'completed_with_issues',
    )
    expect(resolveNotificationResult('scan.failed', 'success')).toBe('failed')
    expect(resolveNotificationResult('scan.cancelled', 'error')).toBe('cancelled')
  })

  it('falls back to severity for unknown notification types', () => {
    expect(resolveNotificationResult('account.changed', 'warning')).toBe('warning')
    expect(resolveNotificationResult('account.changed', 'error')).toBe('failed')
    expect(resolveNotificationResult('account.changed', 'info')).toBe('information')
  })

  it('supports persisted scan notifications from before summary availability was explicit', () => {
    expect(isScanSummaryAvailable({ status: 'success' })).toBe(true)
    expect(isScanSummaryAvailable({ status: 'failed' })).toBe(false)
    expect(isScanSummaryAvailable({ status: 'success', summary_available: false })).toBe(false)
    expect(isScanSummaryAvailable({ status: 'failed', summary_available: true })).toBe(true)
  })
})
