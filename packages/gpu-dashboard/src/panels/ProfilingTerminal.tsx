import { createSignal, createEffect, onCleanup, onMount, For, Show } from 'solid-js'
import { dashboard } from '../store'

type TermLine = { ts: string; cls: string; text: string }

const PREFILL: TermLine[] = [
  { ts: '[00:00:00.000]', cls: 'term-dim',  text: 'Sentinel profiler v0.9.1 — sm_90a target' },
  { ts: '[00:00:00.012]', cls: 'term-ok',   text: 'Attached to CUDA context (device 0: H100 SXM5)' },
  { ts: '[00:00:00.043]', cls: 'term-kern', text: 'KERNEL matmul_tiled<<<(64,64,1),(32,32,1)>>>' },
  { ts: '[00:00:00.043]', cls: 'term-dim',  text: '  registers=64  smem=8192B  occ=50%' },
  { ts: '[00:00:00.185]', cls: 'term-warn', text: 'WARN  L2 cache miss rate 41% (threshold 30%)' },
  { ts: '[00:00:00.186]', cls: 'term-kern', text: 'KERNEL softmax_fwd<<<(128,1,1),(256,1,1)>>>' },
  { ts: '[00:00:00.187]', cls: 'term-dim',  text: '  registers=32  smem=2048B  occ=87%' },
  { ts: '[00:00:00.201]', cls: 'term-ok',   text: 'METRIC  kernel_elapsed=142μs  sm_active=94%' },
  { ts: '[00:00:00.320]', cls: 'term-kern', text: 'KERNEL layer_norm<<<(256,1,1),(128,1,1)>>>' },
  { ts: '[00:00:00.321]', cls: 'term-dim',  text: '  registers=48  smem=4096B  occ=75%' },
  { ts: '[00:00:00.400]', cls: 'term-warn', text: 'WARN  bank conflicts detected in shared mem' },
  { ts: '[00:00:00.512]', cls: 'term-ok',   text: 'METRIC  throughput=8.4 TF/s  (peak 9.7 TF/s)' },
]

const KERNELS = ['matmul_tiled', 'softmax_fwd', 'layer_norm', 'flash_attn_fwd', 'gelu_kernel', 'rope_embed']

function stamp(): string {
  const now = new Date()
  return `[${String(now.getHours()).padStart(2, '0')}:${String(now.getMinutes()).padStart(2, '0')}:${String(now.getSeconds()).padStart(2, '0')}.${String(now.getMilliseconds()).padStart(3, '0')}]`
}

export function ProfilingTerminal() {
  const [lines, setLines] = createSignal<TermLine[]>(PREFILL)
  const [tab, setTab] = createSignal<'trace' | 'summary' | 'timeline'>('trace')
  let bodyRef: HTMLDivElement | undefined

  onMount(() => {
    const id = setInterval(() => {
      const k = KERNELS[Math.floor(Math.random() * KERNELS.length)]
      const dur = (80 + Math.random() * 280).toFixed(0)
      const sm = (75 + Math.random() * 22).toFixed(1)
      setLines(prev => {
        const next = [...prev]
        if (Math.random() < 0.15) {
          next.push({ ts: stamp(), cls: 'term-warn', text: `WARN  sm_active=${sm}% below threshold — check occupancy` })
        } else {
          next.push({ ts: stamp(), cls: 'term-kern', text: `KERNEL ${k}<<<grid,block>>>` })
          next.push({ ts: stamp(), cls: 'term-dim', text: `  elapsed=${dur}μs  SM=${sm}%` })
        }
        return next.length > 200 ? next.slice(-200) : next
      })
    }, 1800)
    onCleanup(() => clearInterval(id))
  })

  createEffect(() => {
    lines()
    if (bodyRef) bodyRef.scrollTop = bodyRef.scrollHeight
  })

  return (
    <div class="panel panel-terminal">
      <div class="panel-header">
        <div class="panel-title">
          <span class="dot" />
          Profiling Terminal
        </div>
        <div class="terminal-meta">
          <span class="mono" style={{ 'font-size': '11px', color: 'var(--fg-dim)' }}>
            {dashboard.kernPerSec} kern/s
          </span>
          <div class="terminal-tabs">
            <button class={`terminal-tab ${tab() === 'trace' ? 'active' : ''}`} onClick={() => setTab('trace')}>Trace</button>
            <button class={`terminal-tab ${tab() === 'summary' ? 'active' : ''}`} onClick={() => setTab('summary')}>Summary</button>
            <button class={`terminal-tab ${tab() === 'timeline' ? 'active' : ''}`} onClick={() => setTab('timeline')}>Timeline</button>
          </div>
        </div>
      </div>

      <div class="terminal">
        <div class="terminal-body" ref={bodyRef}>
          <Show when={tab() === 'trace'}>
            <For each={lines()}>
              {(line) => (
                <span class="term-line">
                  <span class="term-ts">{line.ts}</span>{' '}
                  <span class={line.cls}>{line.text}</span>
                </span>
              )}
            </For>
          </Show>
          <Show when={tab() === 'summary'}>
            <span class="term-line term-ok">kernels={lines().filter(l => l.cls === 'term-kern').length}  warnings={lines().filter(l => l.cls === 'term-warn').length}</span>
            <span class="term-line term-dim">peak SM active ≈ 94% · L2 miss hotspot on matmul_tiled</span>
          </Show>
          <Show when={tab() === 'timeline'}>
            <span class="term-line term-dim">timeline view — stream last {Math.min(lines().length, 40)} events</span>
            <For each={lines().slice(-12)}>
              {(line) => (
                <span class="term-line">
                  <span class="term-ts">{line.ts}</span>{' '}
                  <span class={line.cls}>{line.text}</span>
                </span>
              )}
            </For>
          </Show>
        </div>
      </div>
    </div>
  )
}
