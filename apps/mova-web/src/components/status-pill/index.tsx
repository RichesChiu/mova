export type StatusPillTone = 'admin' | 'system-admin' | 'user'

interface StatusPillProps {
  status: string
  tone: StatusPillTone
}

export const StatusPill = ({ status, tone }: StatusPillProps) => (
  <span className={`status-pill status-pill--${tone}`}>{status}</span>
)
