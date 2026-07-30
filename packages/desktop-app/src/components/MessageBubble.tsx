import type { Message } from "../api/types"

interface Props {
  message: Message
}

export function MessageBubble({ message }: Props) {
  const isUser = message.role === "user"

  return (
    <div className={`message ${isUser ? "message--user" : "message--assistant"}`}>
      <div className="message__role">{isUser ? "You" : "Assistant"}</div>
      <div className="message__content">
        {message.content.split("\n").map((line, i) => (
          <span key={i}>
            {line}
            {i < message.content.split("\n").length - 1 && <br />}
          </span>
        ))}
      </div>
      <div className="message__time">
        {new Date(message.timestamp).toLocaleTimeString()}
      </div>
    </div>
  )
}
