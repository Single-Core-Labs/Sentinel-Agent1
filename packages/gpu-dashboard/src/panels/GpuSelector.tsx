import { createSignal, createEffect, onMount, For, Show } from 'solid-js'
import { dashboard } from '../store'
import type { GpuData } from '../types'

const FAMILIES = ['hopper', 'ada', 'blackwell', 'ampere'] as const
const VRAM_TIERS = ['80gb', '40gb', '24gb'] as const

function sparkline(points: number[], color: string) {
  const max = 100
  const pts = points.map((v, i) => `${(i / 19) * 50},${20 - (v / max) * 18}`).join(' ')
  return (
    <svg class="sparkline" viewBox="0 0 50 20" preserveAspectRatio="none">
      <polyline points={pts} fill="none" stroke={color} stroke-width="1.2" stroke-linejoin="round" />
    </svg>
  )
}

export function GpuSelector() {
  const [checkedFamilies, setCheckedFamilies] = createSignal<Set<string>>(new Set(FAMILIES))
  const [checkedVram, setCheckedVram] = createSignal<Set<string>>(new Set(VRAM_TIERS))
  const [sortKey, setSortKey] = createSignal<'name' | 'vram' | 'price' | 'util'>('name')
  const [sortAsc, setSortAsc] = createSignal(true)
  const [sparkHistory, setSparkHistory] = createSignal<Record<string, number[]>>({})

  onMount(() => {
    const init: Record<string, number[]> = {}
    dashboard.gpus.forEach(g => { init[g.id] = Array(20).fill(0) })
    setSparkHistory(init)
  })

  createEffect(() => {
    const current = { ...sparkHistory() }
    dashboard.gpus.forEach(g => {
      if (!current[g.id]) current[g.id] = Array(20).fill(0)
      current[g.id] = [...current[g.id], g.util].slice(-20)
    })
    setSparkHistory(current)
  })

  const toggleFamily = (fam: string) => {
    setCheckedFamilies(prev => {
      const next = new Set(prev)
      if (next.has(fam)) next.delete(fam)
      else next.add(fam)
      return next
    })
  }

  const toggleVram = (tier: string) => {
    setCheckedVram(prev => {
      const next = new Set(prev)
      if (next.has(tier)) next.delete(tier)
      else next.add(tier)
      return next
    })
  }

  const filtered = () => {
    return dashboard.gpus.filter(g => {
      if (!checkedFamilies().has(g.family)) return false
      if (checkedVram().size) {
        const tier = g.vram >= 80 ? '80gb' : g.vram >= 40 ? '40gb' : '24gb'
        if (!checkedVram().has(tier)) return false
      }
      return true
    }).sort((a, b) => {
      const key = sortKey()
      let va: string | number = a[key]
      let vb: string | number = b[key]
      if (typeof va === 'string' && typeof vb === 'string') {
        va = va.toLowerCase(); vb = vb.toLowerCase()
      }
      return sortAsc() ? (va > vb ? 1 : -1) : (va < vb ? 1 : -1)
    })
  }

  const handleSort = (key: 'name' | 'vram' | 'price' | 'util') => {
    if (sortKey() === key) setSortAsc(prev => !prev)
    else { setSortKey(key); setSortAsc(true) }
  }

  return (
    <div class="panel">
      <div class="panel-header">
        <div class="panel-title">
          <span class="dot" />
          GPU Selector
        </div>
        <span class="mono" style={{ 'font-size': '11px', color: 'var(--fg-dim)' }}>
          {dashboard.selectedGpus.size} selected
        </span>
      </div>

      <div class="gpu-filters" style={{ 'margin-bottom': '16px', display: 'flex', 'flex-wrap': 'wrap', gap: '8px' }}>
        <For each={[...FAMILIES]}>
          {(fam) => (
            <label style={{ display: 'flex', 'align-items': 'center', gap: '6px', 'font-size': '11px', cursor: 'pointer' }}>
              <input
                type="checkbox"
                checked={checkedFamilies().has(fam)}
                onChange={() => toggleFamily(fam)}
                style={{ 'accent-color': 'var(--accent)', width: '12px', height: '12px' }}
              />
              {fam.charAt(0).toUpperCase() + fam.slice(1)}
            </label>
          )}
        </For>
        <For each={[...VRAM_TIERS]}>
          {(tier) => (
            <label style={{ display: 'flex', 'align-items': 'center', gap: '6px', 'font-size': '11px', cursor: 'pointer', 'margin-left': '16px' }}>
              <input
                type="checkbox"
                checked={checkedVram().has(tier)}
                onChange={() => toggleVram(tier)}
                style={{ 'accent-color': 'var(--accent)', width: '12px', height: '12px' }}
              />
              {tier.toUpperCase()}
            </label>
          )}
        </For>
      </div>

      <div class="table-wrap">
        <table>
          <thead>
            <tr>
              <th style={{ width: '32px' }} />
              <th class="sortable" onClick={() => handleSort('name')}>GPU</th>
              <th class="sortable" onClick={() => handleSort('vram')}>VRAM</th>
              <th class="sortable" onClick={() => handleSort('price')}>$/hr</th>
              <th class="sortable" onClick={() => handleSort('util')}>Util</th>
              <th>Spark</th>
            </tr>
          </thead>
          <tbody>
            <For each={filtered()}>
              {(gpu: GpuData) => (
                <tr
                  class={dashboard.selectedGpus.has(gpu.id) ? 'selected' : ''}
                  onClick={() => dashboard.toggleGpu(gpu.id)}
                  style={{ cursor: 'pointer' }}
                >
                  <td>
                    <input
                      type="checkbox"
                      checked={dashboard.selectedGpus.has(gpu.id)}
                      onClick={(e) => e.stopPropagation()}
                      style={{ 'accent-color': 'var(--accent)', width: '12px', height: '12px' }}
                    />
                  </td>
                  <td class="gpu-name">
                    {gpu.name}
                    <Show when={!!gpu.isReal}>
                      <span style={{ 'font-size': '9px', color: 'var(--accent)', background: 'rgba(74,222,128,.12)', padding: '1px 5px', 'border-radius': '4px', border: '1px solid rgba(74,222,128,.25)', 'margin-left': '8px' }}>
                        LIVE
                      </span>
                    </Show>
                  </td>
                  <td class="mono">{gpu.vram > 0 ? gpu.vram + ' GB' : '—'}</td>
                  <td class="mono">{gpu.price > 0 ? '$' + gpu.price.toFixed(2) : 'local'}</td>
                  <td class={`mono util-cell ${gpu.util > 85 ? 'high' : gpu.util > 60 ? 'ok' : ''}`}>{gpu.util}%</td>
                  <td>{sparkline(sparkHistory()[gpu.id] || Array(20).fill(0), gpu.util > 80 ? 'var(--accent-amber)' : 'var(--accent)')}</td>
                </tr>
              )}
            </For>
          </tbody>
        </table>
      </div>

      <Show when={dashboard.selectedGpus.size >= 2}>
        <div class="compare-tray visible" style={{ position: 'static', transform: 'none', 'border-top': '1px solid var(--border)', 'margin-top': '16px', 'padding-top': '12px' }}>
          <div style={{ display: 'flex', 'justify-content': 'space-between', 'align-items': 'center', 'margin-bottom': '12px' }}>
            <span class="mono" style={{ 'font-size': '11px', color: 'var(--fg-dim)' }}>Compare {dashboard.selectedGpus.size} GPUs</span>
            <button class="btn btn-ghost" onClick={() => dashboard.clearSelection()} style={{ 'font-size': '11px', padding: '4px 10px' }}>Clear</button>
          </div>
          <div class="compare-grid">
            <For each={dashboard.gpus.filter(g => dashboard.selectedGpus.has(g.id))}>
              {(g: GpuData) => (
                <div class="compare-card">
                  <div class="cc-name">{g.name}</div>
                  <div class="cc-row"><span class="cc-key">VRAM</span><span class="cc-val">{g.vram}GB</span></div>
                  <div class="cc-row"><span class="cc-key">TFLOPS</span><span class="cc-val">{g.tflops}</span></div>
                  <div class="cc-row"><span class="cc-key">$/hr</span><span class="cc-val">${g.price.toFixed(2)}</span></div>
                  <div class="cc-row"><span class="cc-key">Util</span><span class="cc-val">{g.util}%</span></div>
                  <div class="cc-row"><span class="cc-key">Link</span><span class="cc-val">{g.interconnect}</span></div>
                </div>
              )}
            </For>
          </div>
        </div>
      </Show>
    </div>
  )
}
