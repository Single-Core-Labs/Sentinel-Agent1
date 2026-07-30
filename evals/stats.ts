/**
 * Eval Stats Helper
 *
 * Reads the JSONL eval run log and outputs a formatted summary table
 * of pass/fail rates by category and policy tier.
 *
 * Usage:
 *   bun run evals/stats.ts
 *   bun run evals/stats.ts --category sandbox-safety
 */

import * as fs from 'node:fs';
import * as path from 'node:path';

const LOG_FILE = path.resolve(process.cwd(), 'evals/logs/sentinel-evals.jsonl');

interface LogRecord {
  ts: string;
  name: string;
  category: string;
  policy: string;
  status: 'PASS' | 'FAIL';
  durationMs?: number;
  toolCallCount?: number;
  error?: string;
}

function loadRecords(filterCategory?: string): LogRecord[] {
  if (!fs.existsSync(LOG_FILE)) {
    console.error(`No eval log found at ${LOG_FILE}. Run evals first.`);
    process.exit(1);
  }
  const lines = fs.readFileSync(LOG_FILE, 'utf8').trim().split('\n').filter(Boolean);
  const records: LogRecord[] = lines.map((l) => JSON.parse(l));
  if (filterCategory) {
    return records.filter((r) => r.category === filterCategory);
  }
  return records;
}

function printTable(records: LogRecord[]): void {
  // Group by category
  const byCategory: Record<string, { pass: number; fail: number; totalMs: number }> = {};

  for (const r of records) {
    if (!byCategory[r.category]) {
      byCategory[r.category] = { pass: 0, fail: 0, totalMs: 0 };
    }
    if (r.status === 'PASS') byCategory[r.category].pass++;
    else byCategory[r.category].fail++;
    byCategory[r.category].totalMs += r.durationMs ?? 0;
  }

  const total = records.length;
  const passed = records.filter((r) => r.status === 'PASS').length;
  const passRate = total > 0 ? ((passed / total) * 100).toFixed(1) : '0.0';

  console.log('\n╔══════════════════════════════════════════════════════════════╗');
  console.log('║           Sentinel Agent — Eval Results Summary              ║');
  console.log('╚══════════════════════════════════════════════════════════════╝\n');

  console.log(`  Overall: ${passed}/${total} passed (${passRate}%)\n`);

  // Category breakdown
  const colW = [30, 8, 8, 10, 12];
  const header = [
    'Category'.padEnd(colW[0]),
    'PASS'.padEnd(colW[1]),
    'FAIL'.padEnd(colW[2]),
    'Pass%'.padEnd(colW[3]),
    'Avg ms'.padEnd(colW[4]),
  ].join('│ ');
  console.log('  ' + header);
  console.log('  ' + '─'.repeat(colW.reduce((a, b) => a + b + 2, 0)));

  for (const [cat, stats] of Object.entries(byCategory)) {
    const catTotal = stats.pass + stats.fail;
    const catRate = catTotal > 0 ? ((stats.pass / catTotal) * 100).toFixed(1) : '0.0';
    const avgMs = catTotal > 0 ? (stats.totalMs / catTotal).toFixed(0) : '—';
    const icon = stats.fail === 0 ? '✓' : '✗';
    const row = [
      `${icon} ${cat}`.padEnd(colW[0]),
      String(stats.pass).padEnd(colW[1]),
      String(stats.fail).padEnd(colW[2]),
      `${catRate}%`.padEnd(colW[3]),
      avgMs.padEnd(colW[4]),
    ].join('│ ');
    console.log('  ' + row);
  }

  console.log();

  // List failures
  const failures = records.filter((r) => r.status === 'FAIL');
  if (failures.length > 0) {
    console.log(`  ── Failures (${failures.length}) ─────────────────────────────\n`);
    for (const f of failures) {
      console.log(`  ✗ [${f.category}] ${f.name}`);
      if (f.error) console.log(`      Error: ${f.error.slice(0, 120)}`);
    }
    console.log();
  }
}

// Main
const args = process.argv.slice(2);
const catIdx = args.indexOf('--category');
const filterCat = catIdx !== -1 ? args[catIdx + 1] : undefined;

const records = loadRecords(filterCat);
printTable(records);
