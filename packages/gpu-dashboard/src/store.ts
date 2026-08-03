import { createSignal } from 'solid-js'
import { connect, disconnect, sendRpc, subscribe } from './websocket'
import type { GpuData } from './types'

/** Seed catalog mirrored from public/app.js — live GPU prepended on first gpu/query. */
export const SEED_GPUS: GpuData[] = [
  { id: 'h100-sxm5', name: 'H100 SXM5',    family: 'hopper',    vram: 80,  price: 3.20, util: 0, arch: 'Hopper',    interconnect: 'NVLink', tflops: 989  },
  { id: 'h100-pcie', name: 'H100 PCIe',     family: 'hopper',    vram: 80,  price: 2.80, util: 0, arch: 'Hopper',    interconnect: 'PCIe',   tflops: 756  },
  { id: 'rtx4090',   name: 'RTX 4090',      family: 'ada',       vram: 24,  price: 0.74, util: 0, arch: 'Ada',       interconnect: 'PCIe',   tflops: 660  },
  { id: 'rtx4080',   name: 'RTX 4080',      family: 'ada',       vram: 16,  price: 0.54, util: 0, arch: 'Ada',       interconnect: 'PCIe',   tflops: 490  },
  { id: 'b200',      name: 'B200',           family: 'blackwell', vram: 192, price: 5.50, util: 0, arch: 'Blackwell', interconnect: 'NVLink', tflops: 2250 },
  { id: 'gb200',     name: 'GB200 NVL72',   family: 'blackwell', vram: 384, price: 9.80, util: 0, arch: 'Blackwell', interconnect: 'NVLink', tflops: 4500 },
  { id: 'a100-80',   name: 'A100 SXM4 80G', family: 'ampere',    vram: 80,  price: 2.10, util: 0, arch: 'Ampere',    interconnect: 'NVLink', tflops: 312  },
  { id: 'a100-40',   name: 'A100 PCIe 40G', family: 'ampere',    vram: 40,  price: 1.60, util: 0, arch: 'Ampere',    interconnect: 'PCIe',   tflops: 312  },
]

const REAL_GPU_ID = 'real-gpu-local'

export type WsStatus = 'connecting' | 'connected' | 'disconnected' | 'error'
export type ActiveView = 'dashboard' | 'profiler' | 'cluster' | 'chat' | 'llm'

const [gpus, setGpus] = createSignal<GpuData[]>(SEED_GPUS.map(g => ({ ...g })))
const [selectedGpus, setSelectedGpus] = createSignal<Set<string>>(new Set())
const [wsStatus, setWsStatus] = createSignal<WsStatus>('connecting')
const [activeView, setActiveView] = createSignal<ActiveView>('dashboard')
const [kernPerSec, setKernPerSec] = createSignal(0)
const [clock, setClock] = createSignal('--:--:--')
const [virtAllocPct, setVirtAllocPct] = createSignal(64)

function deriveFamily(name: string): GpuData['family'] {
  const n = name.toLowerCase()
  if (n.includes('h100') || n.includes('h200')) return 'hopper'
  if (n.includes('b200') || n.includes('b100') || n.includes('rtx 50')) return 'blackwell'
  if (n.includes('rtx 40') || n.includes('ada')) return 'ada'
  if (n.includes('a100') || n.includes('rtx 30') || n.includes('ampere')) return 'ampere'
  return 'ada'
}

function injectOrUpdateLiveGpu(result: Record<string, unknown>) {
  const rawName = String(result.name ?? '').trim()
  if (!rawName) return

  const util = typeof result.util_gpu === 'number' ? result.util_gpu : 0
  const vramUsed = typeof result.vram_used_gb === 'number' ? result.vram_used_gb : 0
  const vramTotal = typeof result.vram_total_gb === 'number' ? result.vram_total_gb : 0

  if (vramTotal > 0) {
    setVirtAllocPct(Math.round((vramUsed / vramTotal) * 100))
  }

  const family = deriveFamily(rawName)
  const archLabel =
    family === 'hopper' ? 'Hopper' :
    family === 'blackwell' ? 'Blackwell' :
    family === 'ada' ? 'Ada' : 'Ampere'

  setGpus(prev => {
    const next = [...prev]
    const idx = next.findIndex(g => g.id === REAL_GPU_ID)
    const entry: GpuData = {
      id: REAL_GPU_ID,
      name: rawName,
      family,
      vram: Math.round(vramTotal),
      price: 0,
      util: Math.round(util),
      arch: archLabel,
      interconnect: 'PCIe',
      tflops: 0,
      isReal: true,
    }
    if (idx === -1) next.unshift(entry)
    else next[idx] = { ...next[idx], ...entry }
    return next
  })
}

function simulateUtils() {
  setGpus(prev => prev.map(g => {
    if (g.isReal) return g
    const base = ({ hopper: 82, ada: 65, blackwell: 91, ampere: 58 } as const)[g.family] ?? 60
    const noise = (Math.random() - 0.5) * 14
    return { ...g, util: Math.max(0, Math.min(100, Math.round(base + noise))) }
  }))
  setKernPerSec(Math.round(8 + Math.random() * 24))
}

function tickClock() {
  const now = new Date()
  setClock([
    String(now.getHours()).padStart(2, '0'),
    String(now.getMinutes()).padStart(2, '0'),
    String(now.getSeconds()).padStart(2, '0'),
  ].join(':'))
}

/** Call once from App onMount. Returns cleanup. */
export function initDashboard(): () => void {
  setWsStatus('connecting')
  const unsub = subscribe((data: any) => {
    const result = data?.result
    if (!result) return
    if (result.name && (result.util_gpu !== undefined || result.vram_total_gb !== undefined)) {
      injectOrUpdateLiveGpu(result)
      setWsStatus('connected')
    }
  })

  connect()
  sendRpc('gpu/query')
  setWsStatus('connected')

  const poll = setInterval(() => {
    sendRpc('gpu/query')
    // Keep catalog alive even without a live GPU
    simulateUtils()
  }, 2000)

  const clockTimer = setInterval(tickClock, 1000)
  tickClock()
  simulateUtils()

  return () => {
    clearInterval(poll)
    clearInterval(clockTimer)
    unsub()
    disconnect()
  }
}

export const dashboard = {
  get gpus() { return gpus() },
  get selectedGpus() { return selectedGpus() },
  get wsStatus() { return wsStatus() },
  get wsConnected() { return wsStatus() === 'connected' },
  get activeView() { return activeView() },
  get kernPerSec() { return kernPerSec() },
  get clock() { return clock() },
  get virtAllocPct() { return virtAllocPct() },
  setActiveView,
  setVirtAllocPct,
  toggleGpu(id: string) {
    setSelectedGpus(prev => {
      const next = new Set(prev)
      if (next.has(id)) next.delete(id)
      else next.add(id)
      return next
    })
  },
  clearSelection() {
    setSelectedGpus(new Set<string>())
  },
}
