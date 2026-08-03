import { createSignal, Show } from 'solid-js'

const CUDA_SRC = `__global__ void matmul_tiled(
  float* A, float* B,
  float* C, int N)
{
  __shared__ float sA[32][32];
  __shared__ float sB[32][32];
  int tx = threadIdx.x;
  int ty = threadIdx.y;
  float sum = 0.0f;
  for (int t = 0; t < N/32; t++) {
    sA[ty][tx] = A[...];
    sB[ty][tx] = B[...];
    __syncthreads();
    #pragma unroll
    for (int k=0; k<32; k++)
      sum += sA[ty][k]*sB[k][tx];
    __syncthreads();
  }
  C[...] = sum;
}`

const PTX_HTML = `<span class="ptx-cmt">// PTX ISA 8.4 — sm_90a (Hopper)</span>
<span class="ptx-label">.visible .entry</span> <span class="ptx-op">matmul_tiled</span>(
  <span class="ptx-type">.param .u64</span> <span class="ptx-reg">%A</span>,
  <span class="ptx-type">.param .u64</span> <span class="ptx-reg">%B</span>,
  <span class="ptx-type">.param .u64</span> <span class="ptx-reg">%C</span>,
  <span class="ptx-type">.param .u32</span> <span class="ptx-reg">%N</span>
) {
  <span class="ptx-type">.reg .f32</span>   <span class="ptx-reg">%f</span>&lt;64&gt;;
  <span class="ptx-type">.reg .u32</span>   <span class="ptx-reg">%r</span>&lt;32&gt;;
  <span class="ptx-type">.reg .u64</span>   <span class="ptx-reg">%rd</span>&lt;16&gt;;
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

const SASS_HTML = `<span class="ptx-cmt">// SASS — sm_90a cubin disassembly</span>
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

export function PtxDisassembler() {
  const [mode, setMode] = createSignal<'ptx' | 'sass'>('ptx')

  return (
    <div class="panel panel-ptx">
      <div class="panel-header">
        <div class="panel-title">
          <span class="dot" />
          PTX / SASS Disassembler
        </div>
        <div class="asm-toggle" style={{ 'margin-bottom': '0' }}>
          <button class={mode() === 'ptx' ? 'active' : ''} onClick={() => setMode('ptx')}>PTX</button>
          <button class={mode() === 'sass' ? 'active' : ''} onClick={() => setMode('sass')}>SASS</button>
        </div>
      </div>

      <div class="ptx-split">
        <div class="ptx-pane">
          <div class="pane-label">CUDA Source</div>
          <pre class="code-view mono">{CUDA_SRC}</pre>
        </div>
        <div class="ptx-divider" />
        <div class="ptx-pane">
          <div class="pane-label">{mode().toUpperCase()}</div>
          <Show when={mode() === 'ptx'}>
            <pre class="code-view asm-pane" innerHTML={PTX_HTML} />
          </Show>
          <Show when={mode() === 'sass'}>
            <pre class="code-view asm-pane" innerHTML={SASS_HTML} />
          </Show>
          <div class="reg-pressure-strip" title="Register pressure heatmap" />
        </div>
      </div>
    </div>
  )
}
