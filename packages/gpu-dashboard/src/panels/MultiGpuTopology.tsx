import { createSignal, onCleanup, onMount, For } from 'solid-js'
import type { TopoLink, TopoNode } from '../types'

const TOPO_NODES: TopoNode[] = [
  { id: 0, label: 'H100-0', util: 87, temp: 72, x: 0.18, y: 0.25 },
  { id: 1, label: 'H100-1', util: 91, temp: 74, x: 0.50, y: 0.25 },
  { id: 2, label: 'H100-2', util: 78, temp: 68, x: 0.82, y: 0.25 },
  { id: 3, label: 'H100-3', util: 94, temp: 76, x: 0.18, y: 0.75 },
  { id: 4, label: 'H100-4', util: 62, temp: 61, x: 0.50, y: 0.75 },
  { id: 5, label: 'H100-5', util: 85, temp: 70, x: 0.82, y: 0.75 },
]

const TOPO_LINKS: TopoLink[] = [
  { a: 0, b: 1, type: 'nvlink',     bw: 900 },
  { a: 1, b: 2, type: 'nvlink',     bw: 900 },
  { a: 3, b: 4, type: 'nvlink',     bw: 900 },
  { a: 4, b: 5, type: 'nvlink',     bw: 900 },
  { a: 0, b: 3, type: 'nvlink',     bw: 900 },
  { a: 1, b: 4, type: 'nvlink',     bw: 900 },
  { a: 2, b: 5, type: 'nvlink',     bw: 900 },
  { a: 0, b: 2, type: 'infiniband', bw: 400 },
  { a: 3, b: 5, type: 'infiniband', bw: 400 },
]

const LINK_COLORS: Record<TopoLink['type'], string> = {
  nvlink: '#4ADE80',
  pcie: '#8B8FFF',
  infiniband: '#22D3EE',
}

function roundRect(
  ctx: CanvasRenderingContext2D,
  x: number, y: number, w: number, h: number, r: number,
) {
  ctx.beginPath()
  ctx.moveTo(x + r, y)
  ctx.lineTo(x + w - r, y)
  ctx.quadraticCurveTo(x + w, y, x + w, y + r)
  ctx.lineTo(x + w, y + h - r)
  ctx.quadraticCurveTo(x + w, y + h, x + w - r, y + h)
  ctx.lineTo(x + r, y + h)
  ctx.quadraticCurveTo(x, y + h, x, y + h - r)
  ctx.lineTo(x, y + r)
  ctx.quadraticCurveTo(x, y, x + r, y)
  ctx.closePath()
}

export function MultiGpuTopology() {
  const [mode, setMode] = createSignal<'bandwidth' | 'latency'>('bandwidth')
  let canvasRef: HTMLCanvasElement | undefined
  let animOffset = 0
  let lastTime = 0
  let raf = 0

  const draw = (ts: number) => {
    const canvas = canvasRef
    if (!canvas) return
    const ctx = canvas.getContext('2d')
    if (!ctx) return

    animOffset += (ts - lastTime) * 0.06
    lastTime = ts

    const W = canvas.offsetWidth || 400
    const H = canvas.offsetHeight || 280
    canvas.width = W
    canvas.height = H
    ctx.clearRect(0, 0, W, H)

    const topoMode = mode()

    TOPO_LINKS.forEach(lnk => {
      const na = TOPO_NODES[lnk.a]
      const nb = TOPO_NODES[lnk.b]
      const ax = na.x * W, ay = na.y * H
      const bx = nb.x * W, by = nb.y * H
      const color = LINK_COLORS[lnk.type]

      ctx.beginPath()
      ctx.moveTo(ax, ay)
      ctx.lineTo(bx, by)
      ctx.strokeStyle = color
      ctx.lineWidth = 1.5
      ctx.globalAlpha = 0.35
      ctx.stroke()
      ctx.globalAlpha = 1

      const len = Math.hypot(bx - ax, by - ay) || 1
      const t = (animOffset % len) / len
      const px = ax + (bx - ax) * t
      const py = ay + (by - ay) * t
      ctx.beginPath()
      ctx.arc(px, py, 2.5, 0, Math.PI * 2)
      ctx.fillStyle = color
      ctx.shadowBlur = 6
      ctx.shadowColor = color
      ctx.fill()
      ctx.shadowBlur = 0

      if (W > 250) {
        const mx = (ax + bx) / 2, my = (ay + by) / 2
        ctx.font = '8px JetBrains Mono, monospace'
        ctx.fillStyle = color
        ctx.globalAlpha = 0.7
        ctx.fillText(topoMode === 'bandwidth' ? `${lnk.bw} GB/s` : '1.2μs', mx + 2, my - 2)
        ctx.globalAlpha = 1
      }
    })

    TOPO_NODES.forEach(node => {
      const x = node.x * W, y = node.y * H
      const r = Math.min(W, H) * 0.07

      roundRect(ctx, x - r, y - r * 0.7, r * 2, r * 1.4, 5)
      ctx.fillStyle = '#1A1C20'
      ctx.strokeStyle = node.util > 88 ? '#F5A623' : '#4ADE80'
      ctx.lineWidth = 1.5
      ctx.fill()
      ctx.stroke()

      ctx.beginPath()
      ctx.arc(x + r * 0.55, y - r * 0.3, r * 0.28, -Math.PI / 2, -Math.PI / 2 + (node.util / 100) * Math.PI * 2)
      ctx.strokeStyle = node.util > 88 ? '#F5A623' : '#4ADE80'
      ctx.lineWidth = 2.5
      ctx.stroke()

      ctx.font = '9px JetBrains Mono, monospace'
      ctx.fillStyle = '#F5F5F5'
      ctx.textAlign = 'center'
      ctx.fillText(node.label, x, y + 2)
      ctx.font = '8px JetBrains Mono, monospace'
      ctx.fillStyle = node.temp > 73 ? '#F5A623' : '#9A9AA0'
      ctx.fillText(`${node.temp}°C`, x, y + 13)
    })

    ctx.textAlign = 'left'
    raf = requestAnimationFrame(draw)
  }

  onMount(() => {
    raf = requestAnimationFrame(draw)
    onCleanup(() => cancelAnimationFrame(raf))
  })

  return (
    <div class="panel panel-topology">
      <div class="panel-header">
        <div class="panel-title">
          <span class="dot" />
          Multi-GPU Topology
        </div>
        <div class="asm-toggle" style={{ 'margin-bottom': '0' }}>
          <button class={mode() === 'bandwidth' ? 'active' : ''} onClick={() => setMode('bandwidth')}>Bandwidth</button>
          <button class={mode() === 'latency' ? 'active' : ''} onClick={() => setMode('latency')}>Latency</button>
        </div>
      </div>

      <div class="topology-wrap">
        <canvas class="topo-canvas" ref={canvasRef} />
        <div class="topo-legend">
          <For each={Object.entries(LINK_COLORS)}>
            {([type, color]) => (
              <div class="legend-row">
                <div class="legend-line" style={{ background: color }} />
                <span>{type.toUpperCase()}</span>
              </div>
            )}
          </For>
        </div>
      </div>
    </div>
  )
}
