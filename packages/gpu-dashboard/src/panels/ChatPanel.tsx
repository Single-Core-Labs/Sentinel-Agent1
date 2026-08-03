import { createSignal, onMount, For } from 'solid-js'
import { dashboard } from '../store'
import { sendRpc } from '../websocket'

const CHAT_RESPONSES: Record<string, [string, string]> = {
  'node 3':   ['Checking node 3 status…', 'Node 3 (H100-3) appears idle — last job completed 4m ago. Utilization: <span class="metric-chip amber">GPU Util: 2%</span> Power state: P8 (low-power). Scheduler shows no pending allocation. Recommend running <code>sentinel alloc --node 3</code> to assign next workload.'],
  'memory':   ['Analyzing memory across cluster…', 'Aggregate VRAM: 480/640 GB allocated <span class="metric-chip">75% full</span>. Node 4 has highest pressure at <span class="metric-chip amber">VRAM: 38/40 GB</span>. Suggest migrating job-4822 to node 0 which has <span class="metric-chip">22 GB free</span>.'],
  'slow':     ['Diagnosing performance regression…', 'Detected throughput drop of ~18% vs. baseline. Root cause: L2 cache miss rate spiked at <span class="metric-chip amber">41%</span> on kernel <code>matmul_tiled</code>. Recommend enabling <code>--cache-policy=evict_last</code> and verifying data layout is row-major.'],
  'default':  ['Querying cluster telemetry…', 'Cluster-01: 6 nodes online, <span class="metric-chip">5/6 active</span>. Aggregate throughput: <span class="metric-chip">8.4 TF/s</span>. No critical alerts. Avg temp: <span class="metric-chip amber">71°C</span>.'],
}

function getChatResponse(q: string): [string, string] {
  const lower = q.toLowerCase()
  if (lower.includes('node 3') || lower.includes('idle')) return CHAT_RESPONSES['node 3']
  if (lower.includes('mem') || lower.includes('vram')) return CHAT_RESPONSES['memory']
  if (lower.includes('slow') || lower.includes('perf') || lower.includes('bottleneck')) return CHAT_RESPONSES['slow']
  return CHAT_RESPONSES['default']
}

type ChatMsg = { role: 'user' | 'bot'; content: string; streaming?: boolean }

export function ChatPanel() {
  const [messages, setMessages] = createSignal<ChatMsg[]>([])
  const [input, setInput] = createSignal('')

  const sendMessage = () => {
    const q = input().trim()
    if (!q) return
    setInput('')
    setMessages(prev => [...prev, { role: 'user', content: q }])
    const [thinking, answer] = getChatResponse(q)
    setMessages(prev => [...prev, { role: 'bot', content: thinking }])
    sendRpc('sentinel/chat', { query: q })
    setTimeout(() => {
      setMessages(prev => [...prev.slice(0, -1), { role: 'bot', content: answer, streaming: true }])
      // Remove streaming flag after a moment
      setTimeout(() => setMessages(prev => prev.map((m, i) => i === prev.length - 1 ? { ...m, streaming: false } : m)), 800)
    }, 700)
  }

  const handleKey = (e: KeyboardEvent) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault()
      sendMessage()
    }
  }

  onMount(() => {
    setMessages([{ role: 'bot', content: 'Connected to <strong>cluster-01</strong>. Ask me anything about your GPU cluster — utilization, memory, job scheduling, or performance.' }])
  })

  return (
    <div class="panel">
      <div class="panel-header">
        <div class="panel-title">
          <span class="dot" />
          Chat with Hardware
        </div>
      </div>
      <div class="panel-content" style={{ display: 'flex', 'flex-direction': 'column', height: '400px' }}>
        <div class="chat-messages" style={{flex: 1}}>
          <For each={messages()}>
            {(msg: ChatMsg) => (
              <div class={`chat-msg ${msg.role}`}>
                <div class={`msg-bubble ${msg.streaming ? 'typing-cursor' : ''}`} innerHTML={msg.content} />
              </div>
            )}
          </For>
        </div>
        <div class="chat-input-area">
          <input
            class="chat-input"
            type="text"
            value={input()}
            onInput={(e) => setInput(e.currentTarget.value)}
            onKeyDown={handleKey}
            placeholder="Ask about GPU cluster..."
          />
          <button class="btn btn-primary chat-send" onClick={sendMessage}>Send</button>
        </div>
      </div>
    </div>
  )
}