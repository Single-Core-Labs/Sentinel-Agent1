import { createSignal, createEffect, onMount, For, Show } from 'solid-js'
import { dashboard } from '../store'

const CUDA_SOURCE = [
  { n: 1,  code: '// Tiled matrix multiply — H100 kernel', ann: null },
  { n: 2,  code: '__global__ void matmul_tiled(float* A, float* B, float* C, int N) {', ann: null },
  { n: 3,  code: '  __shared__ float sA[32][32], sB[32][32];', ann: { time: '0μs', mem: null, cls: 'ann-green', tip: 'Shared mem alloc: 8 KB' } },
  { n: 4,  code: '  int tx = threadIdx.x, ty = threadIdx.y;', ann: null },
  { n: 5,  code: '  int bx = blockIdx.x,  by = blockIdx.y;', ann: null },
  { n: 6,  code: '  float sum = 0.0f;', ann: null },
  { n: 7,  code: '  for (int t = 0; t < N / 32; t++) {', ann: { time: '142μs', mem: null, cls: 'ann-amber', tip: 'Hot loop — 98% of kernel time\ncalls: 512  avg: 142μs' } },
  { n: 8,  code: '    sA[ty][tx] = A[(by*32+ty)*N + t*32+tx];', ann: { time: null, mem: '+2.1 GB', cls: 'ann-red', tip: 'Global mem read — uncoalesced\nL2 miss rate: 41%' } },
  { n: 9,  code: '    sB[ty][tx] = B[(t*32+ty)*N + bx*32+tx];', ann: null },
  { n: 10, code: '    __syncthreads();', ann: { time: '18μs', mem: null, cls: 'ann-amber', tip: 'Warp divergence detected\nsync overhead: 18μs avg' } },
  { n: 11, code: '    for (int k = 0; k < 32; k++) sum += sA[ty][k] * sB[k][tx];', ann: { time: '4μs', mem: null, cls: 'ann-green', tip: 'Vectorized FMA — optimal\nthroughput: 98% peak' } },
  { n: 12, code: '    __syncthreads();', ann: null },
  { n: 13, code: '  }', ann: null },
  { n: 14, code: '  C[(by*32+ty)*N + bx*32+tx] = sum;', ann: { time: '6μs', mem: null, cls: 'ann-green', tip: 'Coalesced store\nbandwidth utilization: 87%' } },
  { n: 15, code: '}', ann: null },
]

export function InlineProfiling() {
  const [activeView, setActiveView] = createSignal<'cuda' | 'ptx' | 'sass'>('cuda')

  return (
    <div class="panel">
      <div class="panel-header">
        <div class="panel-title">
          <span class="dot" />
          Inline Profiling
        </div>
        <div class="asm-toggle">
          <button class={activeView() === 'cuda' ? 'active' : ''} onClick={() => setActiveView('cuda')}>CUDA</button>
          <button class={activeView() === 'ptx' ? 'active' : ''} onClick={() => setActiveView('ptx')}>PTX</button>
          <button class={activeView() === 'sass' ? 'active' : ''} onClick={() => setActiveView('sass')}>SASS</button>
        </div>
      </div>

      <div class="panel-content">
        <Show when={activeView() === 'cuda'}>
          <div class="code-block" id="codeProfilerWrap">
            <For each={CUDA_SOURCE}>
              {(row) => (
                <div class={`code-line-row ${row.ann?.cls === 'ann-red' ? 'crit-line' : ''} ${row.ann?.cls === 'ann-amber' ? 'hot-line' : ''}`}>
                  <span class="code-line-num mono">{row.n}</span>
                  <span class="code-line-code mono" innerHTML={row.code} />
                  <Show when={!!row.ann}>
                    <span class={`line-annotation mono ${row.ann!.cls}`}>{row.ann!.time ?? row.ann!.mem ?? ''}</span>
                  </Show>
                </div>
              )}
            </For>
          </div>
        </Show>
        <Show when={activeView() === 'ptx'}>
          <div class="asm-pane" innerHTML={PTX_TEXT} />
        </Show>
        <Show when={activeView() === 'sass'}>
          <div class="asm-pane" innerHTML={SASS_TEXT} />
        </Show>
      </div>
    </div>
  )
}

const PTX_TEXT = `<span class="ptx-cmt">// PTX ISA 8.4 — sm_90a (Hopper)</span>
<span class="ptx-label">.visible .entry</span> <span class="ptx-op">matmul_tiled</span>(
  <span class="ptx-type">.param .u64</span> <span class="ptx-reg">%A</span>,
  <span class="ptx-type">.param .u64</span> <span class="ptx-reg">%B</span>,
  <span class="ptx-type">.param .u64</span> <span class="ptx-reg">%C</span>,
  <span class="ptx-type">.param .u32</span> <span class="ptx-reg">%N</span>
) {
  <span class="ptx-type">.reg .f32</span>   <span class="ptx-reg">%f</span><64>;
  <span class="ptx-type">.reg .u32</span>   <span class="ptx-reg">%r</span><32>;
  <span class="ptx-type">.reg .u64</span>   <span class="ptx-reg">%rd</span><16>;
  <span class="ptx-type">.shared .align 4 .b8</span> sA[<span class="num">4096</span>];
  <span class="ptx-type">.shared .align 4 .b8</span> sB[<span class="num">4096</span>];

  <span class="ptx-op">ld.param.u64</span>    <span class="ptx-reg">%rd0</span>, [<span class="ptx-reg">%A</span>];
  <span class="ptx-op">cvta.to.global.u64</span> <span class="ptx-reg">%rd1</span>, <span class="ptx-reg">%rd0</span>;
  <span class="ptx-op">mov.u32</span>         <span class="ptx-reg">%r0</span>, <span class="ptx-op">%tid.x</span>;
  <span class="ptx-op">mov.u32</span>         <span class="ptx-reg">%r1</span>, <span class="ptx-op">%tid.y</span>;
<span class="ptx-label">LOOP_TOP:</span>
  <span class="ptx-op">ld.global.f32</span>   <span class="ptx-reg">%f0</span>, [<span class="ptx-reg">%rd1</span>];
  <span class="ptx-op">st.shared.f32</span>   [sA + <span class="ptx-reg">%r2</span>], <span class="ptx-reg">%f0</span>;
  <span class="ptx-op">bar.sync</span>        <span class="num">0</span>;
  <span class="ptx-op">fma.rn.f32</span>      <span class="ptx-reg">%f32</span>, <span class="ptx-reg">%f0</span>, <span class="ptx-reg">%f1</span>, <span class="ptx-reg">%f32</span>;
  <span class="ptx-op">bar.sync</span>        <span class="num">0</span>;
  <span class="ptx-op">bra</span>             <span class="ptx-label">LOOP_TOP</span>;
  <span class="ptx-op">st.global.f32</span>   [<span class="ptx-reg">%rd4</span>], <span class="ptx-reg">%f32</span>;
  <span class="ptx-op">ret</span>;
}`

const SASS_TEXT = `<span class="ptx-cmt">// SASS — sm_90a cubin disassembly</span>
<span class="ptx-label">matmul_tiled:</span>
  <span class="ptx-op">MOV</span>     <span class="ptx-reg">R1</span>, c[<span class="num">0x0</span>][<span class="num">0x28</span>]
  <span class="ptx-op">S2R</span>     <span class="ptx-reg">R4</span>, <span class="ptx-op">SR_TID.X</span>
  <span class="ptx-op">S2R</span>     <span class="ptx-reg">R5</span>, <span class="ptx-op">SR_TID.Y</span>
  <span class="ptx-op">IMAD.MOV.U32</span> <span class="ptx-reg">R6</span>, <span class="ptx-reg">RZ</span>, <span class="ptx-reg">RZ</span>, <span class="num">0x0</span>
<span class="ptx-label">LOOP:</span>
  <span class="ptx-op">LDG.E.SYS</span>   <span class="ptx-reg">R8</span>, [<span class="ptx-reg">R2</span>]     <span class="ptx-cmt">// global → reg</span>
  <span class="ptx-op">STS</span>          [<span class="ptx-reg">R10</span>], <span class="ptx-reg">R8</span>   <span class="ptx-cmt">// reg → smem</span>
  <span class="ptx-op">BAR.SYNC</span>     <span class="num">0x0</span>
  <span class="ptx-op">HFMA2.MMA</span>   <span class="ptx-reg">R16</span>, <span class="ptx-reg">R8</span>, <span class="ptx-reg">R9</span>, <span class="ptx-reg">R16</span>
  <span class="ptx-op">BAR.SYNC</span>     <span class="num">0x0</span>
  <span class="ptx-op">ISETP.NE.AND</span> <span class="ptx-reg">P0</span>, <span class="ptx-reg">PT</span>, <span class="ptx-reg">R3</span>, <span class="ptx-reg">RZ</span>, <span class="ptx-reg">PT</span>
  <span class="ptx-op">@P0 BRA</span>      <span class="ptx-label">LOOP</span>
  <span class="ptx-op">STG.E.SYS</span>   [<span class="ptx-reg">R0</span>], <span class="ptx-reg">R16</span>   <span class="ptx-cmt">// store result</span>
  <span class="ptx-op">EXIT</span>`