import { createSignal, onCleanup, For, Show } from 'solid-js'
import { sendRpc } from '../websocket'

interface LlmModel {
  name: string
  quant: string
  vramUsed: number
  vramTotal: number
}

const LLM_MODELS: LlmModel[] = [
  { name: 'Llama-3-8B',   quant: 'Q4_K_M', vramUsed: 5.2,  vramTotal: 8 },
  { name: 'Mistral-7B',   quant: 'Q5_K_M', vramUsed: 6.1,  vramTotal: 8 },
  { name: 'Llama-3-70B',  quant: 'Q4_0',   vramUsed: 38.5, vramTotal: 80 },
  { name: 'Phi-3-medium', quant: 'Q8_0',   vramUsed: 14.8, vramTotal: 16 },
]

const LLM_RESPONSES = [
  'The key difference between attention mechanisms in transformers is the query-key-value projection. In multi-head attention:\n\n```Q = XW_Q\nK = XW_K\nV = XW_V\nOut = softmax(QK^T / sqrt(d_k)) * V```\n\nOn your H100 with FlashAttention-2, you should see near-linear memory scaling.',
  'For optimal throughput on an 8B model with Q4_K_M quantization, key settings are:\n\n```--ctx-size 8192\n--n-gpu-layers 32\n--tensor-split 1\n--batch-size 512```\n\nExpect ~28 tok/s on a single H100 PCIe.',
  'GGUF vs GPTQ: GGUF runs natively on llama.cpp with CPU+GPU offloading. GPTQ is GPU-only but typically 10-15% faster on equivalent hardware due to optimized CUDA kernels. For your setup (H100), GPTQ is recommended.',
]

type Msg = { role: 'user' | 'bot'; content: string; streaming?: boolean }

export function LocalLLMs() {
  const [model, setModel] = createSignal(LLM_MODELS[0])
  const [messages, setMessages] = createSignal<Msg[]>([
    { role: 'bot', content: `${LLM_MODELS[0].name} loaded · ${LLM_MODELS[0].quant} · ready for inference` },
  ])
  const [input, setInput] = createSignal('')
  const [showSettings, setShowSettings] = createSignal(false)
  const [temp, setTemp] = createSignal(0.7)
  const [topP, setTopP] = createSignal(0.9)
  const [tps, setTps] = createSignal(0)
  const [ctxTokens, setCtxTokens] = createSignal(3200)
  let respIdx = 0
  let streamTimer: ReturnType<typeof setInterval> | undefined

  const selectModel = (m: LlmModel) => {
    setModel(m)
    setMessages(prev => [...prev, { role: 'bot', content: `Switched to ${m.name} · ${m.quant}` }])
  }

  const streamBot = (content: string) => {
    if (streamTimer) clearInterval(streamTimer)
    setMessages(prev => [...prev, { role: 'bot', content: '', streaming: true }])
    let charIdx = 0
    streamTimer = setInterval(() => {
      charIdx += 4
      setTps(18 + Math.floor(Math.random() * 30))
      setCtxTokens(t => t + 4)
      setMessages(prev => {
        const next = [...prev]
        const last = next[next.length - 1]
        next[next.length - 1] = {
          ...last,
          content: content.slice(0, charIdx),
          streaming: charIdx < content.length,
        }
        return next
      })
      if (charIdx >= content.length) {
        if (streamTimer) clearInterval(streamTimer)
        streamTimer = undefined
        setTps(0)
      }
    }, 18)
  }

  onCleanup(() => { if (streamTimer) clearInterval(streamTimer) })

  const send = () => {
    const q = input().trim()
    if (!q) return
    setInput('')
    setMessages(prev => [...prev, { role: 'user', content: q }])
    const resp = LLM_RESPONSES[respIdx % LLM_RESPONSES.length]
    respIdx++
    sendRpc('sentinel/llm_chat', { model: model().name, message: q })
    setTimeout(() => streamBot(resp), 300)
  }

  const handleKey = (e: KeyboardEvent) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault()
      send()
    }
  }

  const vramPct = () => (model().vramUsed / model().vramTotal) * 100

  return (
    <div class="llm-panel-inner">
      <div class="llm-header-bar">
        <div class="llm-model-info">
          <span class="model-name">{model().name}</span>
          <span class="model-quant mono">{model().quant}</span>
          <span class="tps-counter mono">{tps()} tok/s</span>
        </div>
        <button class="btn btn-ghost" onClick={() => setShowSettings(s => !s)}>⚙</button>
      </div>

      <div class="llm-model-row">
        <For each={LLM_MODELS}>
          {(m) => (
            <button
              class={`llm-model-chip ${model().name === m.name ? 'active' : ''}`}
              onClick={() => selectModel(m)}
            >
              {m.name}
            </button>
          )}
        </For>
      </div>

      <Show when={showSettings()}>
        <div class="llm-settings">
          <div class="slider-row">
            <label>Temperature <span class="mono">{temp().toFixed(2)}</span></label>
            <input type="range" min="0" max="2" step="0.05" value={temp()}
              onInput={(e) => setTemp(parseFloat(e.currentTarget.value))} />
          </div>
          <div class="slider-row">
            <label>Top-p <span class="mono">{topP().toFixed(2)}</span></label>
            <input type="range" min="0" max="1" step="0.01" value={topP()}
              onInput={(e) => setTopP(parseFloat(e.currentTarget.value))} />
          </div>
        </div>
      </Show>

      <div class="chat-messages llm-messages">
        <For each={messages()}>
          {(msg) => (
            <div class={`chat-msg ${msg.role}`}>
              <div class={`msg-bubble ${msg.streaming ? 'typing-cursor' : ''}`}>
                <Show when={msg.content.includes('```')} fallback={<span>{msg.content}</span>}>
                  <For each={msg.content.split(/```/)}>
                    {(part, i) => (
                      i() % 2 === 1
                        ? <pre class="code-block">{part.trim()}</pre>
                        : <span>{part}</span>
                    )}
                  </For>
                </Show>
              </div>
            </div>
          )}
        </For>
      </div>

      <div class="llm-status-strip">
        <div class="vram-strip-wrap">
          <span class="strip-label">VRAM</span>
          <div class="vram-bar-track">
            <div class="vram-bar-fill" style={{ width: `${vramPct()}%` }} />
          </div>
          <span class="mono">{model().vramUsed} / {model().vramTotal} GB</span>
        </div>
        <div class="ctx-strip">
          <span class="strip-label mono">{(ctxTokens() / 1000).toFixed(1)}k / 8k tokens</span>
        </div>
      </div>

      <div class="chat-input-area">
        <textarea
          class="llm-input"
          rows={2}
          value={input()}
          onInput={(e) => setInput(e.currentTarget.value)}
          onKeyDown={handleKey}
          placeholder="Message the model…"
        />
        <button class="btn btn-primary chat-send" onClick={send}>Send</button>
      </div>
    </div>
  )
}
