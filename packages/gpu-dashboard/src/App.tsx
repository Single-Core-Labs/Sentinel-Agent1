import { createSignal, onCleanup, onMount, For, Show } from 'solid-js'
import { dashboard, initDashboard, type ActiveView } from './store'
import { GpuSelector } from './panels/GpuSelector'
import { BottleneckAnalyzer } from './panels/BottleneckAnalyzer'
import { InlineProfiling } from './panels/InlineProfiling'
import { PtxDisassembler } from './panels/PtxDisassembler'
import { ProfilingTerminal } from './panels/ProfilingTerminal'
import { MultiGpuTopology } from './panels/MultiGpuTopology'
import { VirtPool } from './panels/VirtPool'
import { ChatPanel } from './panels/ChatPanel'
import { LocalLLMs } from './panels/LocalLLMs'

const TABS: { id: ActiveView; label: string }[] = [
  { id: 'dashboard', label: 'Dashboard' },
  { id: 'profiler',  label: 'Profiler' },
  { id: 'cluster',   label: 'Cluster' },
  { id: 'chat',      label: 'Chat' },
  { id: 'llm',       label: 'Local LLMs' },
]

export function App() {
  const [chatOpen, setChatOpen] = createSignal(false)
  const [llmOpen, setLlmOpen] = createSignal(false)

  onMount(() => {
    const cleanup = initDashboard()
    onCleanup(cleanup)
  })

  const selectTab = (id: ActiveView) => {
    dashboard.setActiveView(id)
    if (id === 'chat') setChatOpen(true)
    if (id === 'llm') setLlmOpen(true)
  }

  return (
    <div class="app-shell">
      <header class="topbar">
        <div class="topbar-left">
          <div class="sentinel-logo">
            <span class="logo-icon">⬡</span>
            <span class="logo-name">SENTINEL</span>
            <span class="logo-sub">GPU Profiler</span>
          </div>
          <nav class="topbar-tabs">
            <For each={TABS}>
              {(tab) => (
                <button
                  class={`tab-btn ${dashboard.activeView === tab.id ? 'active' : ''}`}
                  onClick={() => selectTab(tab.id)}
                >
                  {tab.label}
                </button>
              )}
            </For>
          </nav>
        </div>
        <div class="topbar-right">
          <div class="ws-status">
            <span class={`ws-dot ${dashboard.wsStatus === 'connected' ? 'connected' : dashboard.wsStatus === 'error' ? 'error' : ''}`} />
            <span class="mono" style={{ 'font-size': '11px' }}>
              {dashboard.wsStatus === 'connected' ? 'live' : dashboard.wsStatus === 'error' ? 'error' : 'connecting…'}
            </span>
          </div>
          <div class="topbar-clock mono">{dashboard.clock}</div>
        </div>
      </header>

      <main class="main-grid">
        <aside class="col-left">
          <GpuSelector />
        </aside>

        <div class="col-center">
          <BottleneckAnalyzer />
          <InlineProfiling />
          <PtxDisassembler />
        </div>

        <div class="col-right">
          <ProfilingTerminal />
          <MultiGpuTopology />
          <VirtPool />
        </div>
      </main>

      <Show when={chatOpen()}>
        <div class="slide-overlay" onClick={() => setChatOpen(false)}>
          <div class="slide-panel" onClick={(e) => e.stopPropagation()}>
            <div class="slide-header">
              <span class="panel-title">Chat with Hardware</span>
              <button class="btn btn-ghost" onClick={() => setChatOpen(false)}>✕</button>
            </div>
            <ChatPanel />
          </div>
        </div>
      </Show>

      <Show when={llmOpen()}>
        <div class="slide-overlay" onClick={() => setLlmOpen(false)}>
          <div class="slide-panel slide-panel-wide" onClick={(e) => e.stopPropagation()}>
            <div class="slide-header">
              <span class="panel-title">Local LLMs</span>
              <button class="btn btn-ghost" onClick={() => setLlmOpen(false)}>✕</button>
            </div>
            <LocalLLMs />
          </div>
        </div>
      </Show>

      <div class="fab-cluster">
        <button class="fab" title="Chat with Hardware" onClick={() => setChatOpen(true)}>💬</button>
        <button class="fab" title="Local LLMs" onClick={() => setLlmOpen(true)}>🤖</button>
      </div>
    </div>
  )
}
