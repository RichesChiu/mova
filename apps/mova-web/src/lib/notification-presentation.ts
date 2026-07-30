export type NotificationResult =
  | 'completed'
  | 'completed_with_issues'
  | 'failed'
  | 'cancelled'
  | 'warning'
  | 'information'

const scanResults: Record<string, NotificationResult> = {
  'scan.completed': 'completed',
  'scan.completed_with_issues': 'completed_with_issues',
  'scan.failed': 'failed',
  'scan.cancelled': 'cancelled',
}

export const resolveNotificationResult = (
  notificationType: string,
  severity: string,
): NotificationResult => {
  const scanResult = scanResults[notificationType]
  if (scanResult) {
    return scanResult
  }

  switch (severity) {
    case 'success':
      return 'completed'
    case 'warning':
      return 'warning'
    case 'error':
      return 'failed'
    default:
      return 'information'
  }
}

export const isScanSummaryAvailable = (payload: {
  summary_available?: boolean
  status: string
}): boolean => payload.summary_available ?? payload.status === 'success'
