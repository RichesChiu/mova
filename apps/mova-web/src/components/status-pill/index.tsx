export type StatusPillTone = 'admin' | 'system-admin' | 'user'
export type StatusPillSize = 'compact' | 'default'

interface StatusPillProps {
  size?: StatusPillSize
  status: string
  tone: StatusPillTone
}

export const StatusPill = ({ size = 'default', status, tone }: StatusPillProps) => {
  const sizeClassName = size === 'compact' ? ' status-pill--compact' : ''

  return <span className={`status-pill status-pill--${tone}${sizeClassName}`}>{status}</span>
}
