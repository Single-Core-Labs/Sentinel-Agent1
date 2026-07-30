/**
 * Vitest configuration for Sentinel Agent evaluations.
 * 
 * Run modes:
 *   bun run evals:always     - Only ALWAYS_PASSES evals (fast CI gate)
 *   bun run evals:all        - All evals including USUALLY_PASSES
 *   bun run evals:sandbox    - Only sandbox-safety category
 *   bun run evals:hero       - Only hero-scenario category
 *   bun run evals:tools      - Only tool-use-correctness category
 *   bun run evals:behavioral - Only behavioral category
 */

import { defineConfig } from 'vitest/config';
import { fileURLToPath } from 'node:url';
import * as path from 'node:path';

const __dirname = path.dirname(fileURLToPath(import.meta.url));

export default defineConfig({
  test: {
    // 5-minute timeout per eval — hero scenarios can take up to 4 min
    testTimeout: 300_000,
    hookTimeout: 30_000,

    // Parallel execution: 2 evals at a time to avoid rate-limiting
    pool: 'forks',
    poolOptions: {
      forks: {
        minForks: 1,
        maxForks: 2,
      },
    },

    reporters: ['default', 'json', 'verbose'],
    outputFile: {
      json: 'evals/logs/report.json',
    },

    // Pick up all *.eval.ts files under evals/
    include: ['evals/**/*.eval.ts'],
    exclude: ['**/node_modules/**'],

    environment: 'node',
    globals: true,

    // Alias the eval harness (our test-helper)
    alias: {
      './test-helper.js': path.resolve(__dirname, 'test-helper.ts'),
    },
  },
});
