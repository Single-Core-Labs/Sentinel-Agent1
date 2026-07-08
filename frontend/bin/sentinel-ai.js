#!/usr/bin/env node
import { spawnSync } from 'child_process';
import { fileURLToPath } from 'url';
import { dirname, resolve } from 'path';

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

const frontendDir = resolve(__dirname, '..');
const indexFile = resolve(frontendDir, 'src', 'index.tsx');

// We use tsx to run the typescript file directly.
// This assumes tsx is installed locally in frontend/node_modules.
const tsxBin = resolve(frontendDir, 'node_modules', '.bin', 'tsx');

const result = spawnSync(
  process.platform === 'win32' ? `${tsxBin}.cmd` : tsxBin,
  [indexFile, ...process.argv.slice(2)],
  {
    cwd: frontendDir,
    stdio: 'inherit',
    env: {
      ...process.env,
      FORCE_COLOR: '1'
    }
  }
);

process.exit(result.status ?? 0);
