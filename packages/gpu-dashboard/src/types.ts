export interface GpuData {
  id: string
  name: string
  family: 'hopper' | 'ada' | 'blackwell' | 'ampere'
  vram: number
  price: number
  util: number
  arch: string
  interconnect: string
  tflops: number
  isReal?: boolean
}

export interface PipelineStage {
  label: string
  key: string
  color: string
  pct: number
}

export interface VirtInstance {
  owner: string
  vram: number
  total: number
  hot: boolean
}

export interface TopoNode {
  id: number
  label: string
  util: number
  temp: number
  x: number
  y: number
}

export interface TopoLink {
  a: number
  b: number
  type: 'nvlink' | 'pcie' | 'infiniband'
  bw: number
}

export interface ChatMessage {
  role: 'user' | 'bot'
  content: string
}