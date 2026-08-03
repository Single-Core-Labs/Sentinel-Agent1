import { For } from 'solid-js'
import { dashboard } from '../store'
import type { VirtInstance } from '../types'

const VIRT_INSTANCES: VirtInstance[] = [
  { owner: 'job-4821', vram: 20, total: 80, hot: true },
  { owner: 'job-4822', vram: 12, total: 80, hot: false },
  { owner: 'job-4823', vram: 19, total: 80, hot: true },
  { owner: 'job-4824', vram: 8,  total: 40, hot: false },
  { owner: 'job-4825', vram: 16, total: 80, hot: true },
]

export function VirtPool() {
  const allocPct = () => dashboard.virtAllocPct

  return (
    <div class="panel panel-virt">
      <div class="panel-header">
        <div class="panel-title">
          <span class="dot warn" />
          Remote GPU Virtualization
        </div>
        <div class="alloc-bar-wrap">
          <span class="alloc-label">Pool</span>
          <div class="alloc-bar-track">
            <div class="alloc-bar-fill" style={{ width: `${allocPct()}%` }} />
          </div>
          <span class="alloc-pct mono">{allocPct()}%</span>
        </div>
      </div>

      <div class="virt-pool-wrap">
        <div class="pool-visual">
          <For each={VIRT_INSTANCES}>
            {(vi, i) => {
              const pct = vi.vram / vi.total
              return (
                <div
                  class="pool-layer"
                  style={{
                    width: `${58 + i() * 6}px`,
                    height: `${14 + pct * 18}px`,
                    opacity: String(0.5 + pct * 0.5),
                  }}
                />
              )
            }}
          </For>
        </div>

        <div class="virt-instances">
          <For each={VIRT_INSTANCES}>
            {(vi) => (
              <div class={`virt-instance-card ${vi.hot ? 'hot-migrate' : ''}`}>
                <span class="vi-owner mono">{vi.owner}</span>
                <span class="vi-vram mono">{vi.vram}/{vi.total}GB</span>
                <div class="vi-bar-track">
                  <div class="vi-bar-fill" style={{ width: `${(vi.vram / vi.total) * 100}%` }} />
                </div>
              </div>
            )}
          </For>
        </div>
      </div>
    </div>
  )
}
