export function StatusBadge({ status }) {
  const getStatusConfig = () => {
    switch (status) {
      case 'healthy':
        return {
          bg: 'bg-success/20',
          text: 'text-success',
          dot: 'bg-success',
          label: 'Healthy',
        }
      case 'warning':
        return {
          bg: 'bg-warning/20',
          text: 'text-warning',
          dot: 'bg-warning',
          label: 'Warning',
        }
      case 'error':
        return {
          bg: 'bg-error/20',
          text: 'text-error',
          dot: 'bg-error',
          label: 'Error',
        }
      case 'seq-down':
        return {
          bg: 'bg-error/20',
          text: 'text-error',
          dot: 'bg-error',
          label: 'Seq Down',
        }
      default:
        return {
          bg: 'bg-text-secondary/20',
          text: 'text-text-secondary',
          dot: 'bg-text-secondary',
          label: 'Unknown',
        }
    }
  }

  const config = getStatusConfig()

  return (
    <span
      className={`inline-flex items-center gap-1.5 px-2 py-0.5 rounded-full text-xs font-medium ${config.bg} ${config.text}`}
    >
      <span className={`w-1.5 h-1.5 rounded-full ${config.dot}`} />
      {config.label}
    </span>
  )
}
