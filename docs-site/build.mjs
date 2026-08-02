// docs-site/build.mjs — minimal static documentation site.
// Renders each markdown guide in docs-site/guide into standalone HTML pages
// under docs-site/dist. No dependencies; run with: node docs-site/build.mjs
import { readdirSync, readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = dirname(fileURLToPath(import.meta.url));
const guideDir = join(root, 'guide');
const outDir = join(root, 'dist');

const PAGES = [
  { file: 'quickstart.md', label: 'Quick Start' },
  { file: 'providers.md', label: 'Providers' },
  { file: 'custom-tools.md', label: 'Custom Tools' },
  { file: 'sub-agents.md', label: 'Sub-Agents' },
];
const PAGE_LABEL = Object.fromEntries(PAGES.map((p) => [p.file, p.label]));

const escapeHtml = (s) =>
  s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');

function inlineCode(text) {
  const safe = escapeHtml(text);
  return safe.replace(/`([^`]+)`/g, (m, c) => `<code>${c}</code>`);
}

function renderMarkdown(md) {
  const lines = md.split(/\r?\n/);
  const out = [];
  let i = 0;

  let title = 'Sentinel AI Docs';
  for (const l of lines) {
    if (l.startsWith('# ')) { title = l.slice(2).trim(); break; }
  }

  while (i < lines.length) {
    const line = lines[i];

    if (/^```/.test(line)) {
      const lang = line.slice(3).trim();
      const buf = [];
      i++;
      while (i < lines.length && !/^```/.test(lines[i])) { buf.push(lines[i]); i++; }
      const label = lang ? `<span class="lang">${escapeHtml(lang)}</span>` : '';
      out.push(`<pre>${label}<code>${escapeHtml(buf.join('\n'))}</code></pre>`);
      i++;
      continue;
    }

    const h = /^(#{1,4})\s+(.*)$/.exec(line);
    if (h) {
      const level = h[1].length;
      out.push(`<h${level}>${inlineCode(h[2])}</h${level}>`);
      i++;
      continue;
    }

    if (/^\s*(---|\*\*\*)\s*$/.test(line)) { out.push('<hr/>'); i++; continue; }

    if (line.trim().startsWith('|')) {
      const buf = [];
      while (i < lines.length && lines[i].trim().startsWith('|')) { buf.push(lines[i].trim()); i++; }
      const rows = buf
        .filter((r) => !/^\|[\s:|-]+\|$/.test(r))
        .map((r) => r.replace(/^\||\|$/g, '').split('|').map((c) => inlineCode(c.trim())));
      let html = '';
      if (rows[0]) {
        html += '<table><thead><tr>';
        for (const c of rows[0]) html += `<th>${c}</th>`;
        html += '</tr></thead>';
      }
      html += '<tbody>';
      for (const r of rows.slice(1)) html += `<tr>${r.map((c) => `<td>${c}</td>`).join('')}</tr>`;
      html += '</tbody></table>';
      out.push(html);
      continue;
    }

    if (/^\s*[-*]\s+/.test(line)) {
      const buf = [];
      while (i < lines.length && /^\s*[-*]\s+/.test(lines[i])) { buf.push(lines[i].trim().slice(2).trim()); i++; }
      out.push(`<ul>${buf.map((li) => `<li>${inlineCode(li)}</li>`).join('')}</ul>`);
      continue;
    }

    if (line.trim() === '') { i++; continue; }

    const buf = [line];
    i++;
    while (
      i < lines.length &&
      lines[i].trim() !== '' &&
      !/^```/.test(lines[i]) &&
      !/^#{1,4}\s/.test(lines[i]) &&
      !/^\s*[-*]\s+/.test(lines[i]) &&
      !lines[i].trim().startsWith('|')
    ) { buf.push(lines[i]); i++; }

    let para = inlineCode(buf.join(' '));
    para = para.replace(/\*\*([^*]+)\*\*/g, '<strong>$1</strong>');
    para = para.replace(/\[([^\]]+)\]\((https?:[^)\s]+)\)/g, '<a href="$2">$1</a>');
    out.push(`<p>${para}</p>`);
  }
  return { title, body: out.join('\n') };
}

const css = `
:root{color-scheme:dark}
*{box-sizing:border-box}
body{font-family:ui-monospace,SFMono-Regular,Menlo,Consolas,monospace;
  background:#0d1117;color:#c9d1d9;line-height:1.65;margin:0;padding:0}
.wrap{max-width:860px;margin:0 auto;padding:3rem 1.5rem 5rem}
a{color:#58a6ff;text-decoration:none}a:hover{text-decoration:underline}
h1{color:#f0f6fc;border-bottom:1px solid #21262d;padding-bottom:.4rem;margin-top:2rem}
h2,h3,h4{color:#e6edf3;margin-top:2rem}
code{background:#161b22;border:1px solid #21262d;border-radius:6px;padding:.1em .35em;font-size:.9em}
pre{background:#161b22;border:1px solid #21262d;border-radius:8px;padding:1rem;overflow-x:auto}
pre code{background:none;border:none;padding:0}
pre .lang{display:block;font-size:.75rem;color:#8b949e;margin:0 0 .5rem}
table{border-collapse:collapse;margin:1rem 0;width:100%}
th,td{border:1px solid #21262d;padding:.45rem .7rem;text-align:left}
th{background:#161b22}
hr{border:0;border-top:1px solid #21262d;margin:2rem 0}
.linkbar{display:flex;gap:1.2rem;flex-wrap:wrap;margin-bottom:1.5rem;font-size:.95rem}
.linkbar .active{color:#3fb950;border-bottom:2px solid #3fb950}
footer{margin-top:3.5rem;color:#8b949e;font-size:.8rem}
.repo{font-size:.9rem;color:#8b949e;margin-bottom:2rem}
`;

function navFor(current) {
  return `<div class="linkbar">${PAGES.map((p) => {
    const active = p.file === current;
    return `<a class="${active ? 'active' : ''}" href="${p.file.replace(/\.md$/, '.html')}">${PAGE_LABEL[p.file]}</a>`;
  }).join('')}</div>`;
}

function wrapPage(title, inner) {
  return `<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8"/>
<meta name="viewport" content="width=device-width, initial-scale=1"/>
<title>${escapeHtml(title)} — Sentinel</title>
<style>${css}</style>
</head>
<body>${inner}</body>
</html>`;
}

mkdirSync(outDir, { recursive: true });

const indexNav = navFor('');
const indexBody = `
<div class="wrap">
  <h1>Sentinel AI — Documentation</h1>
  <p class="repo">Source: github.com/Single-Core-Labs/Sentinel-Agent1</p>
  ${indexNav}
  <p>An autonomous coding agent for platform engineering, AIOps, and MLOps
     with deep access to docs, cloud compute, and operations tools.</p>
  <ul>
    <li><a href="quickstart.html">Quick Start</a> — install, keys, interactive &amp; headless use, telemetry</li>
    <li><a href="providers.html">Configuring Providers</a> — env vars, routing, MCP servers</li>
    <li><a href="custom-tools.html">Custom Tools</a> — plugins, policy hooks, GPU tools</li>
    <li><a href="sub-agents.html">Sub-Agents &amp; Sessions</a> — sessions, events, commands</li>
  </ul>
  <footer>Sentinel AI — docs</footer>
</div>`;
writeFileSync(join(outDir, 'index.html'), wrapPage('Documentation', indexBody));

for (const p of PAGES) {
  const md = readFileSync(join(guideDir, p.file), 'utf8');
  const { title, body } = renderMarkdown(md);
  const html = wrapPage(
    title,
    `<div class="wrap">${navFor(p.file)}${body}<footer>Sentinel AI — docs</footer></div>`,
  );
  const slug = p.file.replace(/\.md$/, '');
  writeFileSync(join(outDir, `${slug}.html`), html);
}

console.log(`Built docs-site/dist with ${PAGES.length} guides`);