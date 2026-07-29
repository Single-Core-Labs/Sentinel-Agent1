import { useState, useRef, useEffect } from "react"

interface Props {
  onSend: (text: string) => void
  disabled: boolean
  placeholder?: string
}

export function InputBar({ onSend, disabled, placeholder }: Props) {
  const [text, setText] = useState("")
  const textareaRef = useRef<HTMLTextAreaElement>(null)

  useEffect(() => {
    if (!disabled && textareaRef.current) {
      textareaRef.current.focus()
    }
  }, [disabled])

  const handleSend = () => {
    const trimmed = text.trim()
    if (!trimmed || disabled) return
    onSend(trimmed)
    setText("")
  }

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault()
      handleSend()
    }
  }

  // Auto-resize textarea
  useEffect(() => {
    const el = textareaRef.current
    if (el) {
      el.style.height = "auto"
      el.style.height = Math.min(el.scrollHeight, 200) + "px"
    }
  }, [text])

  return (
    <div className="input-bar">
      <textarea
        ref={textareaRef}
        className="input-bar__textarea"
        value={text}
        onChange={(e) => setText(e.target.value)}
        onKeyDown={handleKeyDown}
        placeholder={placeholder ?? "Type a message... (Enter to send, Shift+Enter for newline)"}
        rows={1}
        disabled={disabled}
      />
      <button
        className="input-bar__send"
        onClick={handleSend}
        disabled={disabled || !text.trim()}
      >
        {disabled ? (
          <span className="spinner" />
        ) : (
          <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
            <path d="M22 2L11 13" />
            <path d="M22 2L15 22L11 13L2 9L22 2Z" />
          </svg>
        )}
      </button>
    </div>
  )
}
