interface StatusPillProps {
  status: string
}

export const StatusPill = ({ status }: StatusPillProps) => {
  const normalized = status.toLowerCase()
  const className =
    normalized === 'success'
      ? 'status-pill status-pill--success'
      : normalized === 'failed'
        ? 'status-pill status-pill--danger'
        : 'status-pill status-pill--neutral'

  return <span className={className}>{status}</span>
}
