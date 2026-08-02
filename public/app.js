/* ═══════════════════════════════════════════════════════════════════════
   SENTINEL GPU PROFILER — app.js
   WebSocket telemetry + all 9 panel controllers
   ═══════════════════════════════════════════════════════════════════════ */

'use strict';

// ─── WebSocket ────────────────────────────────────────────────────────────────
const WS_URL = 'ws://127.0.0.1:9090/ws';
let ws = null;
let wsReconnectTimer = null;
let kernelCount = 0;
let kernelsPerSec = 0;
let kpsTimer = null;

function wsConnect() {
  try {
    ws = new WebSocket(WS_URL);
    ws.onopen = () => {
      setWsStatus('connected');
      if (wsReconnectTimer) { clearTimeout(wsReconnectTimer); wsReconnectTimer = null; }
      termLog('t-ok', 'WebSocket connected to ' + WS_URL);
    };
    ws.onclose = () => {
      setWsStatus('disconnected');
      termLog('t-warn', 'WebSocket disconnected — reconnecting in 3s…');
      wsReconnectTimer = setTimeout(wsConnect, 3000);
    };
    ws.onerror = () => setWsStatus('error');
    ws.onmessage = (ev) => handleWsMessage(ev.data);
  } catch (e) {
    setWsStatus('error');
    wsReconnectTimer = setTimeout(wsConnect, 5000);
  }
}

function setWsStatus(state) {
  const dot   = document.getElementById('wsDot');
  const label = document.getElementById('wsLabel');
  dot.className = 'ws-dot ' + (state === 'connected' ? 'connected' : state === 'error' ? 'error' : '');
  label.textContent = state === 'connected' ? 'live' : state === 'error' ? 'error' : 'reconnecting…';
}

function wsSend(method, params = {}) {
  if (ws && ws.readyState === WebSocket.OPEN) {
    ws.send(JSON.stringify({ jsonrpc: '2.0', id: Date.now(), method, params }));
  }
}

function handleWsMessage(raw) {
  let msg;
  try { msg = JSON.parse(raw); } catch { return; }
  const result = msg.result;
  if (!result) return;

  if (result.type === 'kernel_launch') {
    kernelCount++;
    const { name, duration_us, sm_util, vram_used, arch } = result;
    termLog('t-kern', `[${arch}] ${name}`);
    termLog('t-dim',  `  duration=${duration_us}μs  SM=${sm_util}%  VRAM=${vram_used}MB`);
    updateGpuUtil(arch, sm_util);
  } else if (result.type === 'telemetry') {
    ingestTelemetry(result);
  }
}

// ─── GPU Data ─────────────────────────────────────────────────────────────────
const GPU_DATA = [
  { id: 'h100-sxm5', name: 'H100 SXM5',    family: 'hopper',    vram: 80,  price: 3.20, util: 0, arch: 'Hopper',    interconnect: 'NVLink', tflops: 989  },
  { id: 'h100-pcie', name: 'H100 PCIe',     family: 'hopper',    vram: 80,  price: 2.80, util: 0, arch: 'Hopper',    interconnect: 'PCIe',   tflops: 756  },
  { id: 'rtx4090',   name: 'RTX 4090',      family: 'ada',       vram: 24,  price: 0.74, util: 0, arch: 'Ada',       interconnect: 'PCIe',   tflops: 660  },
  { id: 'rtx4080',   name: 'RTX 4080',      family: 'ada',       vram: 16,  price: 0.54, util: 0, arch: 'Ada',       interconnect: 'PCIe',   tflops: 490  },
  { id: 'b200',      name: 'B200',           family: 'blackwell', vram: 192, price: 5.50, util: 0, arch: 'Blackwell', interconnect: 'NVLink', tflops: 2250 },
  { id: 'gb200',     name: 'GB200 NVL72',   family: 'blackwell', vram: 384, price: 9.80, util: 0, arch: 'Blackwell', interconnect: 'NVLink', tflops: 4500 },
  { id: 'a100-80',   name: 'A100 SXM4 80G', family: 'ampere',    vram: 80,  price: 2.10, util: 0, arch: 'Ampere',    interconnect: 'NVLink', tflops: 312  },
  { id: 'a100-40',   name: 'A100 PCIe 40G', family: 'ampere',    vram: 40,  price: 1.60, util: 0, arch: 'Ampere',    interconnect: 'PCIe',   tflops: 312  },
];

// Sparkline history per GPU (last 20 samples)
const sparkHistory = {};
GPU_DATA.forEach(g => { sparkHistory[g.id] = Array(20).fill(0); });

let selectedGpus = new Set();
let sortKey = 'name';
let sortAsc = true;

// Simulate live utilisation
function simulateLiveMetrics() {
  GPU_DATA.forEach(g => {
    const base = { hopper: 82, ada: 65, blackwell: 91, ampere: 58 }[g.family] ?? 60;
    const noise = (Math.random() - 0.5) * 14;
    g.util = Math.max(0, Math.min(100, Math.round(base + noise)));
    sparkHistory[g.id].push(g.util);
    sparkHistory[g.id].shift();
  });
  renderGpuTable();
  kernelsPerSec = Math.round(8 + Math.random() * 24);
  const el = document.getElementById('fpsMeter');
  if (el) el.textContent = kernelsPerSec + ' kern/s';
}

function updateGpuUtil(arch, util) {
  GPU_DATA.filter(g => g.arch === arch).forEach(g => {
    g.util = util;
    sparkHistory[g.id].push(util);
    sparkHistory[g.id].shift();
  });
  renderGpuTable();
}

function ingestTelemetry(t) {
  if (t.vram_alloc_pct != null) {
    const bar = document.getElementById('virtAllocBar');
    const pct = document.getElementById('virtAllocPct');
    if (bar) bar.style.width = t.vram_alloc_pct + '%';
    if (pct) pct.textContent = t.vram_alloc_pct + '%';
  }
}

// ─── Panel 1: GPU Selector ────────────────────────────────────────────────────
function renderGpuTable() {
  const body = document.getElementById('gpuTableBody');
  if (!body) return;

  // Filter
  const checkedFamilies = Array.from(
    document.querySelectorAll('.gpu-filters input[value]:checked')
  ).map(el => el.value);
  const checkedVram = Array.from(
    document.querySelectorAll('.gpu-filters input[value$="gb"]:checked')
  ).map(el => el.value);

  let rows = GPU_DATA.filter(g => {
    if (!checkedFamilies.includes(g.family)) return false;
    if (checkedVram.length) {
      const tier = g.vram >= 80 ? '80gb' : g.vram >= 40 ? '40gb' : '24gb';
      if (!checkedVram.includes(tier)) return false;
    }
    return true;
  });

  // Sort
  rows.sort((a, b) => {
    let va = a[sortKey], vb = b[sortKey];
    if (typeof va === 'string') va = va.toLowerCase(), vb = vb.toLowerCase();
    return sortAsc ? (va > vb ? 1 : -1) : (va < vb ? 1 : -1);
  });

  body.innerHTML = '';
  rows.forEach(g => {
    const tr = document.createElement('tr');
    if (selectedGpus.has(g.id)) tr.classList.add('selected');

    // Sparkline SVG
    const hist = sparkHistory[g.id];
    const max = 100;
    const pts = hist.map((v, i) => `${(i / 19) * 50},${20 - (v / max) * 18}`).join(' ');
    const svg = `<svg class="sparkline" viewBox="0 0 50 20" preserveAspectRatio="none">
      <polyline points="${pts}" fill="none" stroke="${g.util > 80 ? '#F5A623' : '#4ADE80'}" stroke-width="1.2" stroke-linejoin="round"/>
    </svg>`;

    tr.innerHTML = `
      <td><input type="checkbox" ${selectedGpus.has(g.id) ? 'checked' : ''} data-id="${g.id}" style="accent-color:var(--accent-green);width:12px;height:12px;cursor:pointer" /></td>
      <td class="gpu-name">${g.name}</td>
      <td class="mono">${g.vram} GB</td>
      <td class="mono">$${g.price.toFixed(2)}</td>
      <td class="mono util-cell ${g.util > 85 ? 'glow-amber' : ''}">${g.util}%</td>
      <td>${svg}</td>
    `;

    tr.addEventListener('click', (e) => {
      if (e.target.tagName === 'INPUT') return;
      toggleGpuSelect(g.id);
    });
    tr.querySelector('input[type=checkbox]').addEventListener('change', (e) => {
      e.stopPropagation();
      if (e.target.checked) selectedGpus.add(g.id);
      else selectedGpus.delete(g.id);
      renderGpuTable();
      renderCompareTray();
    });

    body.appendChild(tr);
  });

  document.getElementById('gpuSelectedCount').textContent = selectedGpus.size + ' selected';
  renderCompareTray();
}

function toggleGpuSelect(id) {
  if (selectedGpus.has(id)) selectedGpus.delete(id);
  else selectedGpus.add(id);
  renderGpuTable();
  renderCompareTray();
}

function renderCompareTray() {
  const tray = document.getElementById('compareTray');
  const grid = document.getElementById('compareGrid');
  const cnt  = document.getElementById('compareTrayCount');
  if (!tray || !grid) return;
  if (selectedGpus.size < 2) { tray.classList.add('hidden'); return; }
  tray.classList.remove('hidden');
  cnt.textContent = selectedGpus.size;
  grid.innerHTML = '';
  GPU_DATA.filter(g => selectedGpus.has(g.id)).forEach(g => {
    const card = document.createElement('div');
    card.className = 'compare-card';
    card.innerHTML = `
      <div class="cc-name">${g.name}</div>
      <div class="cc-row"><span class="cc-key">VRAM</span><span class="cc-val">${g.vram}GB</span></div>
      <div class="cc-row"><span class="cc-key">TFLOPS</span><span class="cc-val">${g.tflops}</span></div>
      <div class="cc-row"><span class="cc-key">$/hr</span><span class="cc-val">$${g.price.toFixed(2)}</span></div>
      <div class="cc-row"><span class="cc-key">Util</span><span class="cc-val">${g.util}%</span></div>
      <div class="cc-row"><span class="cc-key">Link</span><span class="cc-val">${g.interconnect}</span></div>
    `;
    grid.appendChild(card);
  });
}

function initGpuSelector() {
  document.querySelectorAll('.gpu-table thead th.sortable').forEach(th => {
    th.addEventListener('click', () => {
      const key = th.dataset.sort;
      if (sortKey === key) sortAsc = !sortAsc;
      else { sortKey = key; sortAsc = true; }
      renderGpuTable();
    });
  });
  document.getElementById('clearCompare').addEventListener('click', () => {
    selectedGpus.clear();
    renderGpuTable();
  });
  document.querySelectorAll('.gpu-filters input').forEach(cb => {
    cb.addEventListener('change', renderGpuTable);
  });
  renderGpuTable();
}

// ─── Panel 2: AI Bottleneck Analyzer ─────────────────────────────────────────
const PIPELINE_STAGES = [
  { label: 'Data Load',  key: 'data_load',  color: '#3B82F6' },
  { label: 'Preprocess', key: 'preprocess', color: '#8B8FFF' },
  { label: 'Forward',    key: 'forward',    color: '#22D3EE' },
  { label: 'Backward',   key: 'backward',   color: '#4ADE80' },
  { label: 'Optimizer',  key: 'optimizer',  color: '#A78BFA' },
  { label: 'Sync',       key: 'sync',       color: '#6B7280' },
];

let currentPipeline = null;

function generatePipeline() {
  // Generate realistic-looking timing ratios that sum to 1
  const raw = PIPELINE_STAGES.map(() => Math.random());
  // Occasionally spike one stage to simulate a bottleneck
  const spikeIdx = Math.floor(Math.random() * PIPELINE_STAGES.length);
  raw[spikeIdx] *= 3.5 + Math.random() * 2;
  const total = raw.reduce((a, b) => a + b, 0);
  return raw.map((v, i) => ({
    ...PIPELINE_STAGES[i],
    pct: Math.round((v / total) * 1000) / 10,
  }));
}

function renderBottleneck(stages) {
  const bar    = document.getElementById('pipelineBar');
  const labels = document.getElementById('pipelineLabels');
  const diag   = document.getElementById('diagnosisText');
  const trend  = document.getElementById('bottleneckTrend');
  if (!bar || !labels || !diag) return;

  const maxPct = Math.max(...stages.map(s => s.pct));
  const bottleneck = stages.find(s => s.pct === maxPct);

  bar.innerHTML = '';
  labels.innerHTML = '';

  stages.forEach(s => {
    const seg = document.createElement('div');
    seg.className = 'pipe-seg' + (s === bottleneck ? ' bottleneck' : '');
    seg.style.flex = s.pct;
    if (s !== bottleneck) seg.style.background = s.color;
    seg.textContent = s.pct > 8 ? s.label : '';
    seg.title = `${s.label}: ${s.pct}%`;
    bar.appendChild(seg);

    const lbl = document.createElement('div');
    lbl.className = 'pipe-label-seg';
    lbl.style.flex = s.pct;
    lbl.textContent = s.pct.toFixed(1) + '%';
    labels.appendChild(lbl);
  });

  // Auto-generated diagnosis
  const suggestions = {
    data_load:  `${bottleneck.pct}% of step time in data loading — increase num_workers or prefetch_factor`,
    preprocess: `${bottleneck.pct}% in preprocessing — consider GPU-side augmentation (DALI/cuCIM)`,
    forward:    `${bottleneck.pct}% in forward pass — profile layer-wise with torch.profiler`,
    backward:   `${bottleneck.pct}% in backward pass — check gradient accumulation or mixed precision`,
    optimizer:  `${bottleneck.pct}% in optimizer step — try fused AdamW or ZeroRedundancyOptimizer`,
    sync:       `${bottleneck.pct}% in NCCL sync — reduce all-reduce frequency or use gradient compression`,
  };
  diag.textContent = suggestions[bottleneck.key] ?? `Bottleneck: ${bottleneck.label} (${bottleneck.pct}%)`;

  // Trend arrow
  if (currentPipeline) {
    const prevMax = Math.max(...currentPipeline.map(s => s.pct));
    if (maxPct > prevMax + 2) { trend.textContent = '↑'; trend.className = 'trend-arrow up'; }
    else if (maxPct < prevMax - 2) { trend.textContent = '↓'; trend.className = 'trend-arrow down'; }
    else { trend.textContent = '→'; trend.className = 'trend-arrow'; }
  }
  currentPipeline = stages;
}

function initBottleneck() {
  document.getElementById('runAnalysis').addEventListener('click', () => {
    renderBottleneck(generatePipeline());
    wsSend('sentinel/analyze_bottleneck', { target: 'current_job' });
  });
  // Initial render
  renderBottleneck(generatePipeline());
}

// ─── Panel 4: Inline Profiling ────────────────────────────────────────────────
const CUDA_SOURCE = [
  { n: 1,  code: '<span class="cm">// Tiled matrix multiply — H100 kernel</span>', ann: null },
  { n: 2,  code: '<span class="kw">__global__</span> <span class="kw2">void</span> <span class="fn">matmul_tiled</span>(<span class="kw2">float</span>* A, <span class="kw2">float</span>* B, <span class="kw2">float</span>* C, <span class="kw2">int</span> N) {', ann: null },
  { n: 3,  code: '  <span class="kw">__shared__</span> <span class="kw2">float</span> <span class="nm">sA</span>[<span class="num">32</span>][<span class="num">32</span>], <span class="nm">sB</span>[<span class="num">32</span>][<span class="num">32</span>];', ann: { time: '0μs', mem: null, cls: 'ann-green', tip: 'Shared mem alloc: 8 KB' } },
  { n: 4,  code: '  <span class="kw2">int</span> tx = threadIdx.x, ty = threadIdx.y;', ann: null },
  { n: 5,  code: '  <span class="kw2">int</span> bx = blockIdx.x,  by = blockIdx.y;', ann: null },
  { n: 6,  code: '  <span class="kw2">float</span> sum = <span class="num">0.0f</span>;', ann: null },
  { n: 7,  code: '  <span class="kw">for</span> (<span class="kw2">int</span> t = <span class="num">0</span>; t < N / <span class="num">32</span>; t++) {', ann: { time: '142μs', mem: null, cls: 'ann-amber', tip: 'Hot loop — 98% of kernel time\ncalls: 512  avg: 142μs' } },
  { n: 8,  code: '    sA[ty][tx] = A[(by*<span class="num">32</span>+ty)*N + t*<span class="num">32</span>+tx];', ann: { time: null, mem: '+2.1 GB', cls: 'ann-red', tip: 'Global mem read — uncoalesced\nL2 miss rate: 41%' } },
  { n: 9,  code: '    sB[ty][tx] = B[(t*<span class="num">32</span>+ty)*N + bx*<span class="num">32</span>+tx];', ann: { time: null, mem: null, cls: null, tip: null } },
  { n: 10, code: '    <span class="fn">__syncthreads</span>();', ann: { time: '18μs', mem: null, cls: 'ann-amber', tip: 'Warp divergence detected\nsync overhead: 18μs avg' } },
  { n: 11, code: '    <span class="kw">for</span> (<span class="kw2">int</span> k = <span class="num">0</span>; k < <span class="num">32</span>; k++) sum += sA[ty][k] * sB[k][tx];', ann: { time: '4μs', mem: null, cls: 'ann-green', tip: 'Vectorized FMA — optimal\nthroughput: 98% peak' } },
  { n: 12, code: '    <span class="fn">__syncthreads</span>();', ann: null },
  { n: 13, code: '  }', ann: null },
  { n: 14, code: '  C[(by*<span class="num">32</span>+ty)*N + bx*<span class="num">32</span>+tx] = sum;', ann: { time: '6μs', mem: null, cls: 'ann-green', tip: 'Coalesced store\nbandwidth utilization: 87%' } },
  { n: 15, code: '}', ann: null },
];

function renderInlineProfiling() {
  const wrap = document.getElementById('codeProfilerWrap');
  if (!wrap) return;
  let html = '<div class="profiler-code-block">';
  CUDA_SOURCE.forEach(row => {
    let rowClass = 'code-line-row';
    if (row.ann?.cls === 'ann-red')   rowClass += ' crit-line';
    if (row.ann?.cls === 'ann-amber') rowClass += ' hot-line';
    html += `<div class="${rowClass}">`;
    html += `<span class="line-num">${row.n}</span>`;
    html += `<span class="line-code">${row.code}</span>`;
    if (row.ann && row.ann.cls) {
      const label = row.ann.time ?? row.ann.mem ?? '';
      html += `<span class="line-annotation ${row.ann.cls}">${label}</span>`;
      if (row.ann.tip) {
        html += `<div class="ann-tooltip"><div class="tt-title">Profile Detail</div><div class="tt-val">${row.ann.tip.replace(/\n/g, '<br>')}</div></div>`;
      }
    }
    html += '</div>';
  });
  html += '</div>';
  wrap.innerHTML = html;
}

function initInlineProfiling() {
  renderInlineProfiling();
  ['toggleProfilingView', 'toggleProfilingView2', 'toggleProfilingView3'].forEach(id => {
    const btn = document.getElementById(id);
    if (!btn) return;
    btn.addEventListener('click', () => {
      document.querySelectorAll('[id^="toggleProfilingView"]').forEach(b => b.classList.remove('active-toggle'));
      btn.classList.add('active-toggle');
      // In a real implementation this would swap the source view to PTX/SASS
    });
  });
}

// ─── Panel 8: PTX / SASS Disassembler ────────────────────────────────────────
const CUDA_SRC_TEXT = `__global__ void matmul_tiled(
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
}`;

const PTX_TEXT = `<span class="ptx-cmt">// PTX ISA 8.4 — sm_90a (Hopper)</span>
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
}`;

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
  <span class="ptx-op">EXIT</span>`;

let currentAsmMode = 'ptx';

function renderPtxPanel() {
  const cudaEl = document.getElementById('cudaSource');
  const asmEl  = document.getElementById('asmSource');
  const label  = document.getElementById('asmPaneLabel');
  if (!cudaEl || !asmEl) return;
  cudaEl.innerHTML = escHtml(CUDA_SRC_TEXT);
  asmEl.innerHTML  = currentAsmMode === 'ptx' ? PTX_TEXT : SASS_TEXT;
  if (label) label.textContent = currentAsmMode.toUpperCase();
}

function escHtml(s) {
  return s.replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;');
}

function initPtxPanel() {
  document.getElementById('ptxToggle').addEventListener('click', () => {
    currentAsmMode = 'ptx';
    document.getElementById('ptxToggle').classList.add('active');
    document.getElementById('sassToggle').classList.remove('active');
    renderPtxPanel();
  });
  document.getElementById('sassToggle').addEventListener('click', () => {
    currentAsmMode = 'sass';
    document.getElementById('sassToggle').classList.add('active');
    document.getElementById('ptxToggle').classList.remove('active');
    renderPtxPanel();
  });
  renderPtxPanel();
}

// ─── Panel 7: Profiling Terminal ──────────────────────────────────────────────
const TERM_PREFILL = [
  ['t-ts', '[00:00:00.000]', 't-dim',  'Sentinel profiler v0.9.1 — sm_90a target'],
  ['t-ts', '[00:00:00.012]', 't-ok',   'Attached to CUDA context (device 0: H100 SXM5)'],
  ['t-ts', '[00:00:00.043]', 't-kern', 'KERNEL matmul_tiled<<<(64,64,1),(32,32,1)>>>'],
  ['t-ts', '[00:00:00.043]', 't-dim',  '  registers=64  smem=8192B  occ=50%'],
  ['t-ts', '[00:00:00.185]', 't-warn', 'WARN  L2 cache miss rate 41% (threshold 30%)'],
  ['t-ts', '[00:00:00.186]', 't-kern', 'KERNEL softmax_fwd<<<(128,1,1),(256,1,1)>>>'],
  ['t-ts', '[00:00:00.187]', 't-dim',  '  registers=32  smem=2048B  occ=87%'],
  ['t-ts', '[00:00:00.201]', 't-ok',   'METRIC  kernel_elapsed=142μs  sm_active=94%'],
  ['t-ts', '[00:00:00.320]', 't-kern', 'KERNEL layer_norm<<<(256,1,1),(128,1,1)>>>'],
  ['t-ts', '[00:00:00.321]', 't-dim',  '  registers=48  smem=4096B  occ=75%'],
  ['t-ts', '[00:00:00.400]', 't-warn', 'WARN  bank conflicts detected in shared mem'],
  ['t-ts', '[00:00:00.512]', 't-ok',   'METRIC  throughput=8.4 TF/s  (peak 9.7 TF/s)'],
];

let termLineCount = TERM_PREFILL.length;

function termLog(cls, text) {
  const output = document.getElementById('termOutput');
  if (!output) return;
  const now = new Date();
  const ts = `[${String(now.getHours()).padStart(2,'0')}:${String(now.getMinutes()).padStart(2,'0')}:${String(now.getSeconds()).padStart(2,'0')}.${String(now.getMilliseconds()).padStart(3,'0')}]`;
  const line = document.createElement('span');
  line.className = 'term-line';
  line.innerHTML = `<span class="t-ts">${ts}</span> <span class="${cls}">${escHtml(text)}</span>`;
  output.appendChild(line);
  // Auto-scroll
  const body = document.getElementById('terminalBody');
  if (body) body.scrollTop = body.scrollHeight;
  // Trim to last 200 lines
  while (output.children.length > 200) output.removeChild(output.firstChild);
  termLineCount++;
}

function initTerminal() {
  // Prefill
  TERM_PREFILL.forEach(([tsCls, ts, cls, text]) => {
    const output = document.getElementById('termOutput');
    if (!output) return;
    const line = document.createElement('span');
    line.className = 'term-line';
    line.innerHTML = `<span class="${tsCls}">${ts}</span> <span class="${cls}">${text}</span>`;
    output.appendChild(line);
  });

  // Tab switching
  document.querySelectorAll('.term-tab').forEach(tab => {
    tab.addEventListener('click', () => {
      document.querySelectorAll('.term-tab').forEach(t => t.classList.remove('active'));
      tab.classList.add('active');
    });
  });

  // Simulate streaming profiler output
  setInterval(() => {
    const kernels = ['matmul_tiled', 'softmax_fwd', 'layer_norm', 'flash_attn_fwd', 'gelu_kernel', 'rope_embed'];
    const k = kernels[Math.floor(Math.random() * kernels.length)];
    const dur = (80 + Math.random() * 280).toFixed(0);
    const sm  = (75 + Math.random() * 22).toFixed(1);
    if (Math.random() < 0.15) {
      termLog('t-warn', `WARN  sm_active=${sm}% below threshold — check occupancy`);
    } else {
      termLog('t-kern', `KERNEL ${k}<<<grid,block>>>`);
      termLog('t-dim',  `  elapsed=${dur}μs  SM=${sm}%`);
    }
  }, 1800);
}

// ─── Panel 6: Multi-GPU Topology ─────────────────────────────────────────────
const TOPO_NODES = [
  { id: 0, label: 'H100-0', util: 87, temp: 72, x: 0.18, y: 0.25 },
  { id: 1, label: 'H100-1', util: 91, temp: 74, x: 0.50, y: 0.25 },
  { id: 2, label: 'H100-2', util: 78, temp: 68, x: 0.82, y: 0.25 },
  { id: 3, label: 'H100-3', util: 94, temp: 76, x: 0.18, y: 0.75 },
  { id: 4, label: 'H100-4', util: 62, temp: 61, x: 0.50, y: 0.75 },
  { id: 5, label: 'H100-5', util: 85, temp: 70, x: 0.82, y: 0.75 },
];

const TOPO_LINKS = [
  { a: 0, b: 1, type: 'nvlink',    bw: 900  },
  { a: 1, b: 2, type: 'nvlink',    bw: 900  },
  { a: 3, b: 4, type: 'nvlink',    bw: 900  },
  { a: 4, b: 5, type: 'nvlink',    bw: 900  },
  { a: 0, b: 3, type: 'nvlink',    bw: 900  },
  { a: 1, b: 4, type: 'nvlink',    bw: 900  },
  { a: 2, b: 5, type: 'nvlink',    bw: 900  },
  { a: 0, b: 2, type: 'infiniband',bw: 400  },
  { a: 3, b: 5, type: 'infiniband',bw: 400  },
];

const LINK_COLORS = {
  nvlink:     '#4ADE80',
  pcie:       '#8B8FFF',
  infiniband: '#22D3EE',
};

let topoMode = 'bandwidth';
let topoAnimOffset = 0;

function drawTopology() {
  const canvas = document.getElementById('topoCanvas');
  if (!canvas) return;
  const ctx = canvas.getContext('2d');
  const W = canvas.offsetWidth;
  const H = canvas.offsetHeight;
  canvas.width  = W;
  canvas.height = H;
  ctx.clearRect(0, 0, W, H);

  // Draw links
  TOPO_LINKS.forEach(lnk => {
    const na = TOPO_NODES[lnk.a];
    const nb = TOPO_NODES[lnk.b];
    const ax = na.x * W, ay = na.y * H;
    const bx = nb.x * W, by = nb.y * H;

    ctx.beginPath();
    ctx.moveTo(ax, ay);
    ctx.lineTo(bx, by);
    ctx.strokeStyle = LINK_COLORS[lnk.type] ?? '#555';
    ctx.lineWidth = 1.5;
    ctx.globalAlpha = 0.35;
    ctx.stroke();
    ctx.globalAlpha = 1;

    // Animated dash for active links
    const len = Math.hypot(bx - ax, by - ay);
    const dashPos = (topoAnimOffset % len);
    const t = dashPos / len;
    const px = ax + (bx - ax) * t;
    const py = ay + (by - ay) * t;
    ctx.beginPath();
    ctx.arc(px, py, 2.5, 0, Math.PI * 2);
    ctx.fillStyle = LINK_COLORS[lnk.type] ?? '#555';
    ctx.shadowBlur = 6;
    ctx.shadowColor = LINK_COLORS[lnk.type] ?? '#555';
    ctx.fill();
    ctx.shadowBlur = 0;

    // Label: bandwidth or latency
    if (W > 250) {
      const mx = (ax + bx) / 2, my = (ay + by) / 2;
      ctx.font = '8px JetBrains Mono, monospace';
      ctx.fillStyle = LINK_COLORS[lnk.type];
      ctx.globalAlpha = 0.7;
      ctx.fillText(topoMode === 'bandwidth' ? lnk.bw + ' GB/s' : '1.2μs', mx + 2, my - 2);
      ctx.globalAlpha = 1;
    }
  });

  // Draw nodes
  TOPO_NODES.forEach(node => {
    const x = node.x * W, y = node.y * H;
    const r = Math.min(W, H) * 0.07;

    // Node box
    ctx.beginPath();
    roundRect(ctx, x - r, y - r * 0.7, r * 2, r * 1.4, 5);
    ctx.fillStyle = '#1A1C20';
    ctx.strokeStyle = node.util > 88 ? '#F5A623' : '#4ADE80';
    ctx.lineWidth = 1.5;
    ctx.fill();
    ctx.stroke();

    // Util ring (tiny donut approximation)
    ctx.beginPath();
    ctx.arc(x + r * 0.55, y - r * 0.3, r * 0.28, -Math.PI / 2, -Math.PI / 2 + (node.util / 100) * Math.PI * 2);
    ctx.strokeStyle = node.util > 88 ? '#F5A623' : '#4ADE80';
    ctx.lineWidth = 2.5;
    ctx.stroke();

    // Labels
    ctx.font = '9px JetBrains Mono, monospace';
    ctx.fillStyle = '#F5F5F5';
    ctx.textAlign = 'center';
    ctx.fillText(node.label, x, y + 2);
    ctx.font = '8px JetBrains Mono, monospace';
    ctx.fillStyle = node.temp > 73 ? '#F5A623' : '#9A9AA0';
    ctx.fillText(node.temp + '°C', x, y + 13);
  });

  ctx.textAlign = 'left';
}

function roundRect(ctx, x, y, w, h, r) {
  ctx.beginPath();
  ctx.moveTo(x + r, y);
  ctx.lineTo(x + w - r, y);
  ctx.quadraticCurveTo(x + w, y, x + w, y + r);
  ctx.lineTo(x + w, y + h - r);
  ctx.quadraticCurveTo(x + w, y + h, x + w - r, y + h);
  ctx.lineTo(x + r, y + h);
  ctx.quadraticCurveTo(x, y + h, x, y + h - r);
  ctx.lineTo(x, y + r);
  ctx.quadraticCurveTo(x, y, x + r, y);
  ctx.closePath();
}

function renderTopoLegend() {
  const legend = document.getElementById('topoLegend');
  if (!legend) return;
  legend.innerHTML = Object.entries(LINK_COLORS).map(([type, color]) =>
    `<div class="legend-row">
      <div class="legend-line" style="background:${color}"></div>
      <span>${type.toUpperCase()}</span>
    </div>`
  ).join('');
}

function initTopology() {
  renderTopoLegend();
  document.getElementById('topoViewBw').addEventListener('click', () => {
    topoMode = 'bandwidth';
    document.getElementById('topoViewBw').classList.add('active');
    document.getElementById('topoViewLat').classList.remove('active');
  });
  document.getElementById('topoViewLat').addEventListener('click', () => {
    topoMode = 'latency';
    document.getElementById('topoViewLat').classList.add('active');
    document.getElementById('topoViewBw').classList.remove('active');
  });
  // Animation loop
  let lastTime = 0;
  function topoFrame(ts) {
    topoAnimOffset += (ts - lastTime) * 0.06;
    lastTime = ts;
    drawTopology();
    requestAnimationFrame(topoFrame);
  }
  requestAnimationFrame(topoFrame);
}

// ─── Panel 9: Remote GPU Virtualization ──────────────────────────────────────
const VIRT_INSTANCES = [
  { owner: 'job-4821', vram: 20, total: 80, hot: true  },
  { owner: 'job-4822', vram: 12, total: 80, hot: false },
  { owner: 'job-4823', vram: 19, total: 80, hot: true  },
  { owner: 'job-4824', vram: 8,  total: 40, hot: false },
  { owner: 'job-4825', vram: 16, total: 80, hot: true  },
];

function renderVirtPool() {
  const wrap = document.getElementById('virtPoolWrap');
  if (!wrap) return;

  // Pool visual (stacked layers)
  const totalVram = VIRT_INSTANCES.reduce((s, v) => s + v.vram, 0);
  const maxVram   = VIRT_INSTANCES.reduce((s, v) => s + v.total, 0);
  const allocPct  = Math.round((totalVram / maxVram) * 100);

  const poolEl = document.createElement('div');
  poolEl.className = 'pool-visual';
  VIRT_INSTANCES.forEach((vi, i) => {
    const layer = document.createElement('div');
    layer.className = 'pool-layer';
    const pct = vi.vram / vi.total;
    layer.style.cssText = `width:${58 + i * 6}px; height:${14 + pct * 18}px; opacity:${0.5 + pct * 0.5};`;
    poolEl.appendChild(layer);
  });
  // Allocation bar update
  const bar = document.getElementById('virtAllocBar');
  const pct = document.getElementById('virtAllocPct');
  if (bar) bar.style.width = allocPct + '%';
  if (pct) pct.textContent  = allocPct + '%';

  // Instance cards
  const instEl = document.createElement('div');
  instEl.className = 'virt-instances';
  VIRT_INSTANCES.forEach(vi => {
    const pctFill = (vi.vram / vi.total) * 100;
    const card = document.createElement('div');
    card.className = 'virt-instance-card' + (vi.hot ? ' hot-migrate' : '');
    card.innerHTML = `
      <span class="vi-owner mono">${vi.owner}</span>
      <span class="vi-vram mono">${vi.vram}/${vi.total}GB</span>
      <div class="vi-bar-track"><div class="vi-bar-fill" style="width:${pctFill}%"></div></div>
    `;
    instEl.appendChild(card);
  });

  wrap.innerHTML = '';
  wrap.appendChild(poolEl);
  wrap.appendChild(instEl);
}

// ─── Panel 3: Chat with Hardware ─────────────────────────────────────────────
const CHAT_RESPONSES = {
  'node 3':   ['Checking node 3 status…', 'Node 3 (H100-3) appears idle — last job completed 4m ago. Utilization: <span class="metric-chip amber">GPU Util: 2%</span> Power state: P8 (low-power). Scheduler shows no pending allocation. Recommend running `sentinel alloc --node 3` to assign next workload.'],
  'memory':   ['Analyzing memory across cluster…', 'Aggregate VRAM: 480/640 GB allocated <span class="metric-chip">75% full</span>. Node 4 has highest pressure at <span class="metric-chip amber">VRAM: 38/40 GB</span>. Suggest migrating job-4822 to node 0 which has <span class="metric-chip">22 GB free</span>.'],
  'slow':     ['Diagnosing performance regression…', 'Detected throughput drop of ~18% vs. baseline. Root cause: L2 cache miss rate spiked at <span class="metric-chip amber">41%</span> on kernel `matmul_tiled`. Recommend enabling `--cache-policy=evict_last` and verifying data layout is row-major.'],
  'default':  ['Querying cluster telemetry…', 'Cluster-01: 6 nodes online, <span class="metric-chip">5/6 active</span>. Aggregate throughput: <span class="metric-chip">8.4 TF/s</span>. No critical alerts. Avg temp: <span class="metric-chip amber">71°C</span>.'],
};

function getChatResponse(q) {
  const lower = q.toLowerCase();
  if (lower.includes('node 3') || lower.includes('idle')) return CHAT_RESPONSES['node 3'];
  if (lower.includes('mem') || lower.includes('vram')) return CHAT_RESPONSES['memory'];
  if (lower.includes('slow') || lower.includes('perf') || lower.includes('bottleneck')) return CHAT_RESPONSES['slow'];
  return CHAT_RESPONSES['default'];
}

function appendChatMsg(container, role, html, streaming = false) {
  const wrap = document.createElement('div');
  wrap.className = `chat-msg ${role}`;
  const bubble = document.createElement('div');
  bubble.className = 'msg-bubble';
  if (streaming) {
    bubble.innerHTML = '';
    bubble.classList.add('typing-cursor');
    wrap.appendChild(bubble);
    container.appendChild(wrap);
    container.scrollTop = container.scrollHeight;

    let i = 0;
    const stripped = html.replace(/<[^>]+>/g, '');
    const interval = setInterval(() => {
      i += 3;
      // Show real HTML after stripping for char-by-char effect
      const visibleText = stripped.slice(0, i);
      bubble.textContent = visibleText;
      container.scrollTop = container.scrollHeight;
      if (i >= stripped.length) {
        clearInterval(interval);
        bubble.classList.remove('typing-cursor');
        bubble.innerHTML = html; // restore rich HTML
        container.scrollTop = container.scrollHeight;
      }
    }, 22);
  } else {
    bubble.innerHTML = html;
    wrap.appendChild(bubble);
    container.appendChild(wrap);
    container.scrollTop = container.scrollHeight;
  }
}

function initChat() {
  const overlay  = document.getElementById('chatOverlay');
  const input    = document.getElementById('chatInput');
  const messages = document.getElementById('chatMessages');

  document.getElementById('openChatBtn').addEventListener('click', () => overlay.classList.remove('hidden'));
  document.getElementById('closeChatBtn').addEventListener('click', () => overlay.classList.add('hidden'));

  function sendMsg() {
    const q = input.value.trim();
    if (!q) return;
    appendChatMsg(messages, 'user', q);
    input.value = '';
    const [thinking, answer] = getChatResponse(q);
    appendChatMsg(messages, 'bot', thinking);
    setTimeout(() => appendChatMsg(messages, 'bot', answer, true), 700);
    wsSend('sentinel/chat', { query: q });
  }

  document.getElementById('chatSendBtn').addEventListener('click', sendMsg);
  input.addEventListener('keydown', (e) => { if (e.key === 'Enter') sendMsg(); });

  // Greeting
  appendChatMsg(messages, 'bot', 'Connected to <strong>cluster-01</strong>. Ask me anything about your GPU cluster — utilization, memory, job scheduling, or performance.');
}

// ─── Panel 5: Local LLMs ─────────────────────────────────────────────────────
const LLM_MODELS = [
  { name: 'Llama-3-8B',   quant: 'Q4_K_M', vramUsed: 5.2,  vramTotal: 8 },
  { name: 'Mistral-7B',   quant: 'Q5_K_M', vramUsed: 6.1,  vramTotal: 8 },
  { name: 'Llama-3-70B',  quant: 'Q4_0',   vramUsed: 38.5, vramTotal: 80 },
  { name: 'Phi-3-medium', quant: 'Q8_0',   vramUsed: 14.8, vramTotal: 16 },
];
let currentLlmModel = LLM_MODELS[0];
let llmTpsInterval  = null;
let ctxTokens = 0;

function setLlmModel(model) {
  currentLlmModel = model;
  document.getElementById('llmModelName').textContent = model.name;
  document.getElementById('llmModelQuant').textContent = model.quant;
  const vramPct = (model.vramUsed / model.vramTotal) * 100;
  document.getElementById('llmVramBar').style.width = vramPct + '%';
  document.getElementById('llmVramText').textContent = `${model.vramUsed} / ${model.vramTotal} GB`;
}

function appendLlmMsg(container, role, content, streaming = false) {
  const wrap = document.createElement('div');
  wrap.className = `llm-msg ${role}`;
  const bubble = document.createElement('div');
  bubble.className = 'msg-bubble';

  if (streaming) {
    bubble.classList.add('typing-cursor');
    wrap.appendChild(bubble);
    container.appendChild(wrap);
    container.scrollTop = container.scrollHeight;

    let tps = 0;
    const tpsEl = document.getElementById('llmTps');
    let charIdx = 0;
    llmTpsInterval = setInterval(() => {
      charIdx += 4;
      tps = 18 + Math.floor(Math.random() * 30);
      if (tpsEl) tpsEl.textContent = tps + ' tok/s';
      bubble.textContent = content.slice(0, charIdx);
      // Update context counter
      ctxTokens += 4;
      const ctxEl = document.getElementById('llmCtxText');
      if (ctxEl) ctxEl.textContent = `${(ctxTokens / 1000).toFixed(1)}k / 8k tokens`;
      container.scrollTop = container.scrollHeight;
      if (charIdx >= content.length) {
        clearInterval(llmTpsInterval);
        bubble.classList.remove('typing-cursor');
        if (tpsEl) tpsEl.textContent = '0 tok/s';
      }
    }, 18);
  } else {
    // Check for code blocks
    const codeRx = /```([\s\S]*?)```/g;
    const parts = content.split(codeRx);
    parts.forEach((part, i) => {
      if (i % 2 === 1) {
        const pre = document.createElement('pre');
        pre.className = 'code-block';
        pre.textContent = part.trim();
        bubble.appendChild(pre);
      } else if (part) {
        const span = document.createElement('span');
        span.textContent = part;
        bubble.appendChild(span);
      }
    });
    wrap.appendChild(bubble);
    container.appendChild(wrap);
    container.scrollTop = container.scrollHeight;
  }
}

const LLM_RESPONSES = [
  'The key difference between attention mechanisms in transformers is the query-key-value projection. In multi-head attention:\n\n```Q = XW_Q\nK = XW_K\nV = XW_V\nOut = softmax(QK^T / sqrt(d_k)) * V```\n\nOn your H100 with FlashAttention-2, you should see near-linear memory scaling.',
  'For optimal throughput on an 8B model with Q4_K_M quantization, key settings are:\n\n```--ctx-size 8192\n--n-gpu-layers 32\n--tensor-split 1\n--batch-size 512```\n\nExpect ~28 tok/s on a single H100 PCIe.',
  'GGUF vs GPTQ: GGUF runs natively on llama.cpp with CPU+GPU offloading. GPTQ is GPU-only but typically 10-15% faster on equivalent hardware due to optimized CUDA kernels. For your setup (H100), GPTQ is recommended.',
];
let llmRespIdx = 0;

function initLlm() {
  const overlay   = document.getElementById('llmOverlay');
  const input     = document.getElementById('llmInput');
  const messages  = document.getElementById('llmMessages');

  document.getElementById('openLlmBtn').addEventListener('click', () => overlay.classList.remove('hidden'));
  document.getElementById('closeLlmBtn').addEventListener('click', () => overlay.classList.add('hidden'));

  // Settings toggle
  document.getElementById('llmSettingsBtn').addEventListener('click', () => {
    document.getElementById('llmSettings').classList.toggle('hidden');
  });
  document.getElementById('tempSlider').addEventListener('input', (e) => {
    document.getElementById('tempVal').textContent = parseFloat(e.target.value).toFixed(2);
  });
  document.getElementById('topPSlider').addEventListener('input', (e) => {
    document.getElementById('topPVal').textContent = parseFloat(e.target.value).toFixed(2);
  });

  function sendLlm() {
    const q = input.value.trim();
    if (!q) return;
    appendLlmMsg(messages, 'user', q);
    input.value = '';
    const resp = LLM_RESPONSES[llmRespIdx % LLM_RESPONSES.length];
    llmRespIdx++;
    setTimeout(() => appendLlmMsg(messages, 'bot', resp, true), 300);
    wsSend('sentinel/llm_chat', { model: currentLlmModel.name, message: q });
  }

  document.getElementById('llmSendBtn').addEventListener('click', sendLlm);
  input.addEventListener('keydown', (e) => { if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); sendLlm(); } });

  setLlmModel(currentLlmModel);
  appendLlmMsg(messages, 'bot', `${currentLlmModel.name} loaded · ${currentLlmModel.quant} · ready for inference`);
}

// ─── Clock ───────────────────────────────────────────────────────────────────
function updateClock() {
  const el = document.getElementById('topbarClock');
  if (!el) return;
  const now = new Date();
  el.textContent = [
    String(now.getHours()).padStart(2, '0'),
    String(now.getMinutes()).padStart(2, '0'),
    String(now.getSeconds()).padStart(2, '0'),
  ].join(':');
}

// ─── Topbar nav ───────────────────────────────────────────────────────────────
function initNav() {
  document.querySelectorAll('.tab-btn').forEach(btn => {
    btn.addEventListener('click', () => {
      document.querySelectorAll('.tab-btn').forEach(b => b.classList.remove('active'));
      btn.classList.add('active');
      // For future multi-view support — all panels visible in this single-page build
    });
  });
}

// ─── Boot ─────────────────────────────────────────────────────────────────────
document.addEventListener('DOMContentLoaded', () => {
  // Initialise all panels
  initNav();
  initGpuSelector();
  initBottleneck();
  initInlineProfiling();
  initPtxPanel();
  initTerminal();
  initTopology();
  renderVirtPool();
  initChat();
  initLlm();

  // Clock
  updateClock();
  setInterval(updateClock, 1000);

  // Live GPU metrics simulation (every 2s)
  simulateLiveMetrics();
  setInterval(simulateLiveMetrics, 2000);

  // WebSocket (connect after short delay so UI is painted first)
  setTimeout(wsConnect, 400);

  console.log('%c SENTINEL GPU PROFILER ', 'background:#131417;color:#4ADE80;font-family:monospace;font-size:14px;padding:4px 8px;border:1px solid #4ADE80');
  console.log('%c WS → ws://127.0.0.1:9090/ws ', 'color:#9A9AA0;font-family:monospace');
});
