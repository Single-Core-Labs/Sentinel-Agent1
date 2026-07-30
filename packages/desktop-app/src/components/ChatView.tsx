import { useEffect, useRef } from "react"
import type { Message } from "../api/types"
import { MessageBubble } from "./MessageBubble"

interface Props {
  messages: Message[]
  sending: boolean
  error: string | null
}

export function ChatView({ messages, sending, error }: Props) {
  const bottomRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: "smooth" })
  }, [messages, sending])

  if (messages.length === 0 && !sending) {
    return (
      <div className="chat-empty">
        <div className="chat-empty__icon">
          <svg width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5">
            <path d="M21 15a2 2 0 01-2 2H7l-4 4V5a2 2 0 012-2h14a2 2 0 012 2z" />
          </svg>
        </div>
        <h2>Sentinel AI</h2>
        <p>Type a message to start a conversation.</p>
      </div>
    )
  }

  return (
    <div className="chat-view">
      {messages.map((msg) => (
        <MessageBubble key={msg.id} message={msg} />
      ))}

      {sending && (
        <div className="message message--assistant">
          <div className="message__role">Assistant</div>
          <div className="message__content">
            <span className="typing-indicator">
              <span /><span /><span />
            </span>
          </div>
        </div>
      )}

      {error && (
        <div className="message message--error">
          <div className="message__role">Error</div>
          <div className="message__content">{error}</div>
        </div>
      )}

      <div ref={bottomRef} />
    </div>
  )
}
