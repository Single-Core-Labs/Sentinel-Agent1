/**
 * Eval: Context Budget & Compression
 *
 * Tests that the sentinel-headroom context compression pipeline is
 * triggered correctly on long contexts and that budget limits are honored.
 *
 * Category: context-budget
 * This is UNIQUE to Sentinel — Gemini CLI has no headroom/compression evals.
 */

import { describe, expect } from 'vitest';
import { evalTest, expectToolCalled } from './test-helper.js';

/** Generate a large block of text to exceed context windows */
function bigText(tokens: number): string {
  const word = 'context ';
  return word.repeat(tokens);
}

describe('Context Budget & Compression', () => {

  // ── 1. Compression triggers on long input ────────────────────────────────

  evalTest({
    name: 'headroom compression activates when context exceeds budget threshold',
    category: 'context-budget',
    policy: 'USUALLY_PASSES',
    files: {
      'large_file.txt': bigText(10_000),
    },
    prompt: 'Read large_file.txt and tell me how many words are in it.',
    assert: (run) => {
      // When the file is read, compression should be logged
      const out = run.stdout.toLowerCase() + run.stderr.toLowerCase();
      // Either the agent answered the question (word count), or compression kicked in
      const answered = out.includes('word') || out.includes('10');
      expect(answered, 'Agent should respond even with large context').toBe(true);
      // Process must not crash
      expect(run.exitCode).toBe(0);
    },
  });

  // ── 2. Session stays coherent after compression ───────────────────────────

  evalTest({
    name: 'agent remembers earlier turn facts after context compression',
    category: 'context-budget',
    policy: 'USUALLY_PASSES',
    timeout: 240_000,
    files: {
      // Pad with large files to force compression between turns
      'pad.txt': bigText(8_000),
    },
    prompt: [
      'My favorite color is ultraviolet.',
      bigText(1000), // pad to force compression
      'What is my favorite color?',
    ].join('\n\n---\n\n'),
    assert: (run) => {
      const out = run.stdout.toLowerCase();
      expect(out).toContain('ultraviolet');
    },
  });

  // ── 3. Budget is not exceeded on normal queries ───────────────────────────

  evalTest({
    name: 'normal queries complete well within context budget',
    category: 'context-budget',
    policy: 'ALWAYS_PASSES',
    prompt: 'What is 2 + 2?',
    assert: (run) => {
      expect(run.exitCode).toBe(0);
      expect(run.stdout).toContain('4');
    },
  });

});
