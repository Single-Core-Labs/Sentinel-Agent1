#!/usr/bin/env node
import { render } from 'ink';
import fs from 'node:fs';
import path from 'node:path';
import dotenv from 'dotenv';

// Load .env from the project root (one level up from frontend)
dotenv.config({ path: path.resolve(process.cwd(), '../.env') });

import App from './App.js';

// ── Debug-file logger ──────────────────────────────────────────────
// Intercepts console.* calls that would otherwise write directly to
// stdout/stderr, bypassing Ink's frame buffer and causing overlapping.
// In --debug mode they go to a file; otherwise they're suppressed.

const DEBUG = process.argv.includes('--debug');
const LOG_PATH = path.join(
  process.env.XDG_STATE_HOME
    || (process.platform === 'win32'
      ? (process.env.APPDATA || process.env.USERPROFILE || '')
      : path.join(process.env.HOME || '', '.local', 'state')),
  'platform-agent',
  'cli-debug.log',
);

let logStream: fs.WriteStream | null = null;

if (DEBUG) {
  try {
    fs.mkdirSync(path.dirname(LOG_PATH), { recursive: true });
    logStream = fs.createWriteStream(LOG_PATH, { flags: 'a' });
    logStream.write(`\n--- session ${new Date().toISOString()} ---\n`);
  } catch {
    // can't create log dir — suppress and move on
  }
}

function routeToFile(...args: unknown[]) {
  if (logStream) {
    logStream.write(args.map(a => (typeof a === 'string' ? a : JSON.stringify(a))).join(' ') + '\n');
  }
}

console.log = routeToFile;
console.debug = routeToFile;
console.warn = routeToFile;
console.error = routeToFile;

// ── Render ─────────────────────────────────────────────────────────

const { waitUntilExit } = render(<App />);
await waitUntilExit();

// Flush and cleanup
logStream?.end();
