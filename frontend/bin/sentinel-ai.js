#!/usr/bin/env node
import { createRequire } from 'module';
import { fileURLToPath, pathToFileURL } from 'url';
import { dirname, resolve } from 'path';

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const require_ = createRequire(import.meta.url);

require_('tsx');

const frontendDir = resolve(__dirname, '..');
const entryUrl = pathToFileURL(resolve(frontendDir, 'src', 'index.tsx'));
await import(entryUrl);
