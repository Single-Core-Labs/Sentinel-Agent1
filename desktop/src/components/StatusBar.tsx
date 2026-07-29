import type { ConnectionState } from "../api/types"

interface Props {
  connection: ConnectionState
  sessionId: string | null
  model: string
}

export function StatusBar({ connection, sessionId, model }: Props) {
  const dotClass =
    connection.status === "connected"
      ? "status-dot--green"
      : connection.status === "connecting"
        ? "status-dot--yellow"
        : "status-dot--red"

  const label =
    connection.status === "connected"
      ? "Connected"
      : connection.status === "connecting"
        ? "Connecting..."
        : "Disconnected"

  return (
    <div className="status-bar">
      <div className="status-bar__left">
        <span className={`status-dot ${dotClass}`} />
        <span className="status-label">{label}</span>
        {connection.error && (
          <span className="status-error" title={connection.error}>
            Error
          </span>
        )}
      </div>
      <div className="status-bar__right">
        {model && <span className="status-model">{model}</span>}
        {sessionId && (
          <span className="status-session" title={sessionId}>
            {sessionId.slice(0, 8)}...
          </span>
        )}
      </div>
    </div>
  )
}
