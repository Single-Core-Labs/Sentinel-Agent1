/* Sentinel GPU Dashboard – WebSocket JSON‑RPC client */
(() => {
  const wsStatus = document.getElementById('ws-status');
  const wsLabel = document.getElementById('ws-label');
  const telemetryDiv = document.getElementById('telemetry');
  const gpuNameSpan = document.getElementById('gpu-name');
  const gpuUtilSpan = document.getElementById('gpu-util');
  const gpuTempSpan = document.getElementById('gpu-temp');

  const gpuTargets = document.getElementById('gpu-targets');
  const archSelect = document.getElementById('gpu-arch-select');
  const kernelSelect = document.getElementById('kernel-select');
  const customKernelInput = document.getElementById('custom-kernel');
  const loadBtn = document.getElementById('load-kernel');
  const runEmulateBtn = document.getElementById('run-emulate');
  const runProfileBtn = document.getElementById('run-profile');
  const recPre = document.getElementById('rec-content');

  const codeTabs = document.getElementById('code-tabs');
  const codeCuda = document.getElementById('code-cuda');
  const codePtx = document.getElementById('code-ptx');
  const codeSass = document.getElementById('code-sass');

  const metricElems = {
    time: document.getElementById('stat-time'),
    cycles: document.getElementById('stat-cycles'),
    ipc: document.getElementById('stat-ipc'),
    bottleneck: document.getElementById('stat-bottleneck'),
    occupancy: document.getElementById('stat-occupancy'),
    smutil: document.getElementById('stat-smutil'),
    coalesce: document.getElementById('stat-coalesce'),
    region: document.getElementById('stat-region'),
  };

  const gpuSpecPre = document.getElementById('gpu-spec');

  // ---------- WebSocket setup ----------
  let ws;
  let nextId = 1;
  const pending = new Map();

  function setWsStatus(connected) {
    wsStatus.classList.toggle('connected', connected);
    wsLabel.textContent = `WS: ${connected ? 'Connected' : 'Disconnected'}`;
  }

  function connectWs() {
    const url = `ws://${location.host}/ws`;
    ws = new WebSocket(url);
    ws.addEventListener('open', () => setWsStatus(true));
    ws.addEventListener('close', () => setWsStatus(false));
    ws.addEventListener('error', () => setWsStatus(false));
    ws.addEventListener('message', ev => onMessage(ev));
  }

  function onMessage(ev) {
    try {
      const msg = JSON.parse(ev.data);
      if ('id' in msg) {
        const { id } = msg;
        const defer = pending.get(id);
        if (defer) {
          pending.delete(id);
          if (msg.error) defer.reject(msg.error);
          else defer.resolve(msg.result);
        }
      } else if (msg.method === 'event') {
        // currently we ignore server‑sent events for this dashboard
      }
    } catch (e) {
      console.error('Invalid WS message', e);
    }
  }

  function rpc(method, params = {}) {
    return new Promise((resolve, reject) => {
      const id = String(nextId++);
      pending.set(id, { resolve, reject });
      ws.send(JSON.stringify({ jsonrpc: '2.0', id, method, params }));
    });
  }

  // ---------- UI helpers ----------
  function setActiveChip(target) {
    document.querySelectorAll('.chip').forEach(ch => {
      ch.classList.toggle('active', ch.dataset.arch === target);
    });
    // sync select box
    archSelect.value = target;
  }

  function loadSampleKernel(path) {
    // update UI selections
    kernelSelect.value = path;
    customKernelInput.value = '';
    loadKernel(path);
  }

  function loadKernel(path) {
    rpc('fs/readFile', { path }).then(res => {
      const src = res.content;
      codeCuda.textContent = src;
      // reset PTX/SASS panes until we have a report
      codePtx.textContent = '// PTX view will appear after analysis';
      codeSass.textContent = '// SASS view will appear after analysis';
    }).catch(err => alert('Failed to read kernel: ' + err.message));
  }

  // ---------- Parsing helpers ----------
  function parseReport(text) {
    const data = {};
    const lines = text.split('\n');
    let section = '';
    for (const raw of lines) {
      const line = raw.trim();
      if (!line) continue;
      if (line.startsWith('---')) { section = line; continue; }
      const colon = line.indexOf(':');
      if (colon === -1) continue;
      const key = line.slice(0, colon).trim();
      const val = line.slice(colon + 1).trim();
      // capture primary metrics from the Execution section
      switch (key) {
        case 'Estimated Time': data.time_us = parseFloat(val.split(' ')[0]); break;
        case 'Total Cycles': data.cycles = parseInt(val.replace(/,/g, ''), 10); break;
        case 'IPC': data.ipc = parseFloat(val); break;
        case 'Bottleneck': data.bottleneck = val; break;
        case 'Occupancy': data.occupancy = parseFloat(val.replace('%', '')); break;
        case 'SM Util': data.sm_util = parseFloat(val.replace('%', '')); break;
        case 'Coalescing Eff.': data.coalesce = parseFloat(val.replace('%', '')); break;
        case 'Arith. Intensity': data.arith_intensity = parseFloat(val);
          // next line (ridge point) is optional; ignore.
          break;
        case 'Region': data.region = val; break;
        case 'Bottleneck': data.bottleneck = val; break;
        default:
          // capture instruction mix values using known labels
          if (key === 'Arithmetic') data.arith = parseInt(val, 10);
          else if (key === 'Memory (global)') data.mem_global = parseInt(val, 10);
          else if (key === 'Memory (shared)') data.mem_shared = parseInt(val, 10);
          else if (key === 'Tensor Core') data.tensor = parseInt(val, 10);
          else if (key === 'Sync') data.sync = parseInt(val, 10);
          else if (key === 'Branches') data.branches = parseInt(val, 10);
          break;
      }
    }
    // Best Config extraction
    const bestMatch = text.match(/★\s*Best Config:\s*([\w\s]+)\s*\(score:\s*([0-9.]+)\)/);
    if (bestMatch) {
      data.best_config = bestMatch[1].trim();
      data.best_score = parseFloat(bestMatch[2]);
    }
    // Recommendations (lines prefixed with '!')
    const recs = [];
    const recRegex = /^\s*!\s*(.+)$/gm;
    let m;
    while ((m = recRegex.exec(text)) !== null) {
      recs.push(m[1].trim());
    }
    data.recommendations = recs;
    // Profile issues – lines like "L12 [warn] msg → suggestion"
    const issues = [];
    const issueRegex = /^\s*L(\d+) \[(error|warn|info)\] (.+?) → (.+)$/gm;
    while ((m = issueRegex.exec(text)) !== null) {
      issues.push({ line: parseInt(m[1], 10), sev: m[2], msg: m[3].trim(), suggestion: m[4].trim() });
    }
    data.issues = issues;
    return data;
  }

  function synthesizePTX(report) {
    // Very rough PTX from instruction mix – each category becomes a dummy instruction
    const lines = [];
    const add = (cnt, tmpl) => { for (let i = 0; i < cnt; i++) lines.push(tmpl.replace('{i}', i + 1)); };
    if (report.arith) add(report.arith, '    // arithmetic op {i}');
    if (report.mem_global) add(report.mem_global, '    ld.global.u32 r{i}, [addr];');
    if (report.mem_shared) add(report.mem_shared, '    ld.shared.u32 r{i}, [s_addr];');
    if (report.tensor) add(report.tensor, '    turing.tensor.fma.rN.f32 r{i}, r{i}, r{i};');
    if (report.sync) add(report.sync, '    bar.sync 0;');
    if (report.branches) add(report.branches, '    @p{i} bra LABEL{i};');
    return lines.join('\n');
  }

  function synthesizeSASS(report) {
    // Rough SASS – just echo PTX with a comment prefix
    const ptx = synthesizePTX(report);
    return ptx.split('\n').map(l => l.replace(/^\s+/, '    ')).join('\n');
  }

  function updateMetrics(report) {
    metricElems.time.textContent = report.time_us ? `${report.time_us.toFixed(1)} µs` : '–';
    metricElems.cycles.textContent = report.cycles ? report.cycles.toLocaleString() : '–';
    metricElems.ipc.textContent = report.ipc?.toFixed(2) ?? '–';
    metricElems.bottleneck.textContent = report.bottleneck ?? '–';
    metricElems.occupancy.textContent = report.occupancy ? `${report.occupancy.toFixed(0)}%` : '–';
    metricElems.smutil.textContent = report.sm_util ? `${report.sm_util.toFixed(0)}%` : '–';
    metricElems.coalesce.textContent = report.coalesce ? `${report.coalesce.toFixed(0)}%` : '–';
    metricElems.region.textContent = report.region ?? '–';
    // Recommendations
    const recs = [];
    if (report.best_config) recs.push(`★ Best Config: ${report.best_config} (score: ${report.best_score?.toFixed(3)})`);
    recs.push(...(report.recommendations || []));
    recs.push(...(report.issues || []).map(i => `L${i.line} [${i.sev}] ${i.msg} → ${i.suggestion}`));
    recPre.textContent = recs.length ? recs.join('\n') : '–';
    // PTX/SASS panes
    codePtx.textContent = synthesizePTX(report);
    codeSass.textContent = synthesizeSASS(report);
  }

  function updateGpuSpec(archName) {
    // Simple spec table based on known archs (mirrors ARCH_SPECS in Rust)
    const specs = {
      'Pascal61': {name:'GTX 1080 Ti', cc:'6.1', sm:28, bw:484, clock:1582, smem:98},
      'Volta70': {name:'Tesla V100', cc:'7.0', sm:80, bw:900, clock:1530, smem:98},
      'Turing75': {name:'RTX 2080 Ti', cc:'7.5', sm:68, bw:616, clock:1545, smem:64},
      'Ampere80': {name:'A100', cc:'8.0', sm:108, bw:1555, clock:1410, smem:168},
      'Ampere86': {name:'RTX 3090', cc:'8.6', sm:82, bw:936, clock:1695, smem:131},
      'Ada89': {name:'RTX 4090', cc:'8.9', sm:128, bw:1008, clock:2520, smem:131},
      'Hopper90': {name:'H100', cc:'9.0', sm:132, bw:3352, clock:1980, smem:232},
      'Hopper92': {name:'H200', cc:'9.2', sm:132, bw:4800, clock:1980, smem:232},
      'Blackwell100': {name:'RTX 5090', cc:'10.0', sm:170, bw:1792, clock:2520, smem:131},
      'Blackwell102': {name:'B200', cc:'10.2', sm:168, bw:8000, clock:1980, smem:232},
    };
    const spec = specs[archName] || specs['Ampere86'];
    gpuSpecPre.textContent = `${spec.name} (CC ${spec.cc})\nSMs: ${spec.sm}\nClock: ${spec.clock} MHz\nMemory BW: ${spec.bw} GB/s\nShared Mem/SM: ${spec.smem} KB`;
  }

  // ---------- Event listeners ----------
  // chip clicks
  gpuTargets.addEventListener('click', e => {
    const chip = e.target.closest('.chip');
    if (!chip) return;
    const arch = chip.dataset.arch;
    setActiveChip(arch);
    updateGpuSpec(arch);
  });
  archSelect.addEventListener('change', e => {
    const arch = e.target.value;
    setActiveChip(arch);
    updateGpuSpec(arch);
  });

  loadBtn.addEventListener('click', () => {
    const path = customKernelInput.value.trim() || kernelSelect.value;
    if (!path) return;
    loadKernel(path);
  });

  runEmulateBtn.addEventListener('click', async () => {
    const path = customKernelInput.value.trim() || kernelSelect.value;
    const arch = archSelect.value;
    try {
      const res = await rpc('gpu/emulate', { file_path: path, sweep: true, arch });
      const report = parseReport(res.report);
      updateMetrics(report);
    } catch (e) {
      alert('Emulate failed: ' + (e.message || e));
    }
  });

  runProfileBtn.addEventListener('click', async () => {
    const path = customKernelInput.value.trim() || kernelSelect.value;
    const arch = archSelect.value;
    try {
      const res = await rpc('gpu/profile', { file_path: path });
      const report = parseReport(res.report);
      updateMetrics(report);
    } catch (e) {
      alert('Profile failed: ' + (e.message || e));
    }
  });

  // code tab switching
  codeTabs.addEventListener('click', e => {
    const btn = e.target.closest('.tab');
    if (!btn) return;
    const target = btn.dataset.tab;
    document.querySelectorAll('.tab').forEach(t => t.classList.toggle('active', t === btn));
    codeCuda.style.display = target === 'cuda' ? 'block' : 'none';
    codePtx.style.display = target === 'ptx' ? 'block' : 'none';
    codeSass.style.display = target === 'sass' ? 'block' : 'none';
  });

  // ---------- Telemetry polling ----------
  async function pollTelemetry() {
    try {
      const data = await rpc('gpu/query');
      gpuNameSpan.textContent = `GPU: ${data.name || '–'}`;
      gpuUtilSpan.textContent = `Util: ${data.util_gpu?.toFixed(0) ?? '–'}%`;
      gpuTempSpan.textContent = `Temp: ${data.temp_c?.toFixed(0) ?? '–'}°C`;
    } catch (_) {
      // ignore – could be non‑NVIDIA host
    }
    setTimeout(pollTelemetry, 3000);
  }

   // ---------- Init ----------
   connectWs();
   setTimeout(pollTelemetry, 1000);
   // default UI state
   setActiveChip('H100');
   updateGpuSpec('H100');
   // load default kernel
   loadKernel(kernelSelect.value);

   // ---------- GPU Selector Logic ----------
   const GPU_CATALOG = [
     {id:'gpu1', name:'H100', family:'Hopper', vram:80, price:3.2, region:'us-east', available:true},
     {id:'gpu2', name:'A100', family:'Ampere', vram:40, price:2.5, region:'us-west', available:true},
     {id:'gpu3', name:'RTX 4090', family:'Ada', vram:24, price:1.8, region:'eu-central', available:true},
     {id:'gpu4', name:'RTX 3090', family:'Ampere', vram:24, price:1.5, region:'us-east', available:true},
     {id:'gpu5', name:'RTX 3080', family:'Ampere', vram:10, price:0.9, region:'us-west', available:true},
     {id:'gpu6', name:'RTX 2080 Ti', family:'Turing', vram:11, price:0.7, region:'eu-central', available:false},
   ];
   let selectedGPUs = [];
   const gpuListEl = document.getElementById('gpu-list');
   const compareTray = document.getElementById('compare-tray');
   const compareGrid = document.getElementById('compare-grid');

   function applyFilters() {
     const families = Array.from(document.querySelectorAll('.family-filter:checked')).map(cb => cb.value);
     const vramTiers = Array.from(document.querySelectorAll('.vram-filter:checked')).map(cb => cb.value);
     const regions = Array.from(document.querySelectorAll('.region-filter:checked')).map(cb => cb.value);
     const onlyAvail = document.querySelector('.avail-filter').checked;
     return GPU_CATALOG.filter(g => {
       if (!families.includes(g.family)) return false;
       const vt = g.vram < 8 ? '<8' : (g.vram <= 24 ? '8-24' : '>24');
       if (!vramTiers.includes(vt)) return false;
       if (!regions.includes(g.region)) return false;
       if (onlyAvail && !g.available) return false;
       return true;
     });
   }

   function renderGpuList() {
     gpuListEl.innerHTML = '';
     const filtered = applyFilters();
     filtered.forEach(g => {
       const card = document.createElement('div');
       card.className = 'gpu-card';
       card.dataset.id = g.id;
       card.innerHTML = `<div class="title">${g.name}</div>
         <div class="detail">Family: ${g.family}<br>VRAM: ${g.vram} GB<br>Price: $${g.price.toFixed(2)}/hr</div>
         <div class="sparkline" id="spark-${g.id}"></div>`;
       card.addEventListener('click', () => toggleSelect(g.id, card));
       gpuListEl.appendChild(card);
       // initialise sparkline bars (10 bars)
       const sparkDiv = document.getElementById(`spark-${g.id}`);
       for (let i = 0; i < 10; i++) {
         const bar = document.createElement('div');
         bar.style.width = '6%';
         sparkDiv.appendChild(bar);
       }
     });
   }

   function toggleSelect(id, cardEl) {
     const idx = selectedGPUs.indexOf(id);
     if (idx === -1) { selectedGPUs.push(id); cardEl.classList.add('selected'); }
     else { selectedGPUs.splice(idx, 1); cardEl.classList.remove('selected'); }
     updateCompareTray();
   }

   function updateCompareTray() {
     if (selectedGPUs.length >= 2) {
       compareTray.classList.remove('hidden');
       compareGrid.innerHTML = '';
       selectedGPUs.forEach(id => {
         const g = GPU_CATALOG.find(x => x.id === id);
         const c = document.createElement('div');
         c.className = 'compare-card';
         c.innerHTML = `<div class="title">${g.name}</div>
           <div class="detail">Family: ${g.family}<br>VRAM: ${g.vram} GB<br>Price: $${g.price.toFixed(2)}/hr<br>Region: ${g.region}<br>Util: <span id="util-${g.id}">–</span>%</div>`;
         compareGrid.appendChild(c);
       });
     } else {
       compareTray.classList.add('hidden');
     }
   }

   function updateUtilizations() {
     GPU_CATALOG.forEach(g => {
       if (!g.available) return;
       const util = Math.floor(Math.random() * 100);
       const spark = document.getElementById(`spark-${g.id}`);
       if (spark) {
         const bars = spark.children;
         // shift opacity to simulate trail
         for (let i = 0; i < bars.length - 1; i++) bars[i].style.opacity = '0.3';
         const newBar = document.createElement('div');
         newBar.style.flex = '1';
         newBar.style.background = 'var(--accent)';
         newBar.style.opacity = (util / 100).toString();
         spark.appendChild(newBar);
         if (bars.length > 10) spark.removeChild(bars[0]);
       }
     });
     // update comparison tray utilization values
     selectedGPUs.forEach(id => {
       const util = Math.floor(Math.random() * 100);
       const el = document.getElementById(`util-${id}`);
       if (el) el.textContent = util;
     });
   }

   // Filter change listeners
   document.querySelectorAll('.family-filter,.vram-filter,.region-filter,.avail-filter').forEach(ch => {
     ch.addEventListener('change', () => { renderGpuList(); updateCompareTray(); });
   });

   renderGpuList();
   setInterval(updateUtilizations, 2000);

 })();
