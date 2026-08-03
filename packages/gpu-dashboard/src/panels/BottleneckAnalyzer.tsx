import { createSignal, onMount, For } from 'solid-js'
import { sendRpc } from '../websocket'
import type { PipelineStage } from '../types'

const PIPELINE_STAGES = [
  { label: 'Data Load',  key: 'data_load',  color: '#3B82F6' },
  { label: 'Preprocess', key: 'preprocess', color: '#8B8FFF' },
  { label: 'Forward',    key: 'forward',    color: '#22D3EE' },
  { label: 'Backward',   key: 'backward',   color: '#4ADE80' },
  { label: 'Optimizer',  key: 'optimizer',  color: '#A78BFA' },
  { label: 'Sync',       key: 'sync',       color: '#6B7280' },
] as const

const SUGGESTIONS: Record<string, string> = {
  data_load:  'Increase num_workers or prefetch_factor',
  preprocess: 'Consider GPU-side augmentation (DALI/cuCIM)',
  forward:    'Profile layer-wise with torch.profiler',
  backward:   'Check gradient accumulation or mixed precision',
  optimizer:  'Try fused AdamW or ZeroRedundancyOptimizer',
  sync:       'Reduce all-reduce frequency or use gradient compression',
}

function generatePipeline(): PipelineStage[] {
  const raw = PIPELINE_STAGES.map(() => Math.random())
  const spikeIdx = Math.floor(Math.random() * PIPELINE_STAGES.length)
  raw[spikeIdx] *= 3.5 + Math.random() * 2
  const total = raw.reduce((a, b) => a + b, 0)
  return PIPELINE_STAGES.map((s, i) => ({
    ...s,
    pct: Math.round((raw[i] / total) * 1000) / 10,
  }))
}

export function BottleneckAnalyzer() {
  const [stages, setStages] = createSignal<PipelineStage[]>(generatePipeline())
  const [diagnosis, setDiagnosis] = createSignal('Analyzing...')

  const runAnalysis = () => {
    const next = generatePipeline()
    setStages(next)
    const maxPct = Math.max(...next.map(s => s.pct))
    const bottleneck = next.find(s => s.pct === maxPct)
    if (bottleneck) {
      setDiagnosis(`${bottleneck.pct}% of step time in ${bottleneck.label.toLowerCase()} — ${SUGGESTIONS[bottleneck.key]}`)
    }
    sendRpc('sentinel/analyze_bottleneck', { target: 'current_job' })
  }

  onMount(() => {
    runAnalysis()
  })

  return (
    <div class="panel">
      <div class="panel-header">
        <div class="panel-title">
          <span class="dot warn" />
          AI Bottleneck Analyzer
        </div>
        <button class="btn btn-primary" onClick={runAnalysis} style={{ 'font-size': '11px', padding: '6px 12px' }}>
          Run Analysis
        </button>
      </div>

      <div class="panel-content">
        <div class="pipeline-bar">
          <For each={stages()}>
            {(s: PipelineStage) => {
              const isBottleneck = s.pct === Math.max(...stages().map(x => x.pct))
              return (
                <div class={`pipe-seg ${isBottleneck ? 'bottleneck' : ''}`} style={{ flex: s.pct, background: s.color }}>
                  {s.pct > 8 ? s.label : ''}
                </div>
              )
            }}
          </For>
        </div>
        <div class="pipe-labels">
          <For each={stages()}>
            {(s: PipelineStage) => (
              <div class="pipe-label-seg" style={{ flex: s.pct }}>{s.pct.toFixed(1)}%</div>
            )}
          </For>
        </div>

        <div style={{ 'margin-top': '16px', padding: '12px', background: 'var(--bg)', 'border-radius': '6px', border: '1px solid var(--border)' }}>
          <div style={{ 'font-size': '11px', color: 'var(--fg-dim)', 'margin-bottom': '6px', 'font-family': 'var(--mono)' }}>Diagnosis</div>
          <div style={{ 'font-size': '13px', color: 'var(--fg)', 'font-weight': 500 }}>{diagnosis()}</div>
        </div>
      </div>
    </div>
  )
}
