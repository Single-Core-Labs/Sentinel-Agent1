/**
 * Eval: Hero Scenarios
 *
 * End-to-end user journeys that span multiple tools, multiple turns,
 * and validate the full agent value proposition.
 *
 * Category: hero-scenario
 * Improvements over Gemini:
 *  - Realistic developer workflows (debug → fix → test → commit)
 *  - Cross-file refactoring validation
 *  - Multi-step agentic code review
 *  - Provider-agnostic (works with any SENTINEL_EVAL_MODEL)
 */

import { describe, expect } from 'vitest';
import {
  evalTest,
  expectFileExists,
  readWorkspaceFile,
  expectToolCalled,
  expectJudgeYes,
} from './test-helper.js';

describe('Hero Scenarios', () => {

  // ── 1. Debug → Fix → Verify cycle ────────────────────────────────────────

  evalTest({
    name: 'debugs a buggy Python function and fixes it',
    category: 'hero-scenario',
    policy: 'USUALLY_PASSES',
    timeout: 180_000,
    files: {
      'math_utils.py': `
def divide(a, b):
    # BUG: does not handle division by zero
    return a / b

def add(a, b):
    return a + b
`.trim(),
    },
    prompt: 'The function `divide` in math_utils.py crashes when b is 0. Fix it to return None instead of raising an exception. Do not change the add function.',
    assert: async (_run, workDir) => {
      const content = readWorkspaceFile(workDir, 'math_utils.py');
      // Must handle zero
      expect(content).toContain('def divide');
      expect(content).toContain('def add');
      const hasZeroCheck =
        content.includes('b == 0') ||
        content.includes('b is 0') ||
        content.includes('ZeroDivisionError') ||
        content.includes('try') ||
        content.includes('if b');
      expect(hasZeroCheck, 'Expected divide function to handle division by zero').toBe(true);
    },
  });

  // ── 2. Multi-file refactor ────────────────────────────────────────────────

  evalTest({
    name: 'renames a function across multiple files',
    category: 'hero-scenario',
    policy: 'USUALLY_PASSES',
    timeout: 240_000,
    files: {
      'api.py': 'from utils import get_data\n\ndef handler():\n    return get_data()\n',
      'utils.py': 'def get_data():\n    return {"status": "ok"}\n',
    },
    prompt: 'Rename the function `get_data` to `fetch_data` everywhere in this project. Update both utils.py and api.py.',
    assert: async (_run, workDir) => {
      const api = readWorkspaceFile(workDir, 'api.py');
      const utils = readWorkspaceFile(workDir, 'utils.py');

      expect(api).toContain('fetch_data');
      expect(api).not.toContain('get_data');
      expect(utils).toContain('def fetch_data');
      expect(utils).not.toContain('def get_data');
    },
  });

  // ── 3. Generate + run tests ───────────────────────────────────────────────

  evalTest({
    name: 'generates unit tests for a given function',
    category: 'hero-scenario',
    policy: 'USUALLY_PASSES',
    timeout: 180_000,
    files: {
      'calculator.py': `
def multiply(a: int, b: int) -> int:
    return a * b
`.trim(),
    },
    prompt: 'Write pytest unit tests for the `multiply` function in calculator.py. Save them to test_calculator.py. Include tests for positive numbers, zero, and negative numbers.',
    assert: async (_run, workDir) => {
      expectFileExists(workDir, 'test_calculator.py');
      const tests = readWorkspaceFile(workDir, 'test_calculator.py');
      expect(tests).toContain('multiply');
      // Should have multiple test cases
      const testFunctionCount = (tests.match(/def test_/g) || []).length;
      expect(testFunctionCount, 'Expected at least 3 test functions').toBeGreaterThanOrEqual(3);
    },
  });

  // ── 4. Code review & improvement ─────────────────────────────────────────

  evalTest({
    name: 'reviews code and identifies real problems with LLM judge validation',
    category: 'hero-scenario',
    policy: 'USUALLY_PASSES',
    timeout: 180_000,
    files: {
      'server.py': `
import subprocess
import os

def run_command(user_input: str):
    # Security: run user input as shell command
    result = subprocess.run(user_input, shell=True, capture_output=True)
    return result.stdout

def get_secret():
    # Hardcoded credentials
    password = "admin123"
    return password
`.trim(),
    },
    prompt: 'Please review server.py and identify any security vulnerabilities.',
    assert: async (run) => {
      // Use LLM judge to verify the review correctly identified injection + hardcoded creds
      await expectJudgeYes(
        run,
        'Does the agent output identify BOTH a shell injection vulnerability AND a hardcoded credential/password problem in the code?',
        3, // self-consistency: 3 runs, majority vote
      );
    },
  });

  // ── 5. Full git workflow ───────────────────────────────────────────────────

  evalTest({
    name: 'creates feature branch, makes change, and commits with descriptive message',
    category: 'hero-scenario',
    policy: 'USUALLY_PASSES',
    timeout: 240_000,
    files: { 'README.md': '# My Project\n' },
    prompt: 'Create a new git branch called "feature/add-license", add a LICENSE file containing "MIT License", commit it with a descriptive message, and switch back to main.',
    assert: (run, workDir) => {
      expectFileExists(workDir, 'LICENSE');
      const licenseContent = readWorkspaceFile(workDir, 'LICENSE');
      expect(licenseContent.toLowerCase()).toContain('mit');
      // Verify git commands were executed
      const gitCalls = run.toolCalls.filter(
        (c) => c.name === 'run_shell_command' &&
          typeof c.args['command'] === 'string' &&
          (c.args['command'] as string).includes('git'),
      );
      expect(gitCalls.length, 'Expected multiple git commands to be executed').toBeGreaterThanOrEqual(3);
    },
  });

  // ── 6. Explain complex code ───────────────────────────────────────────────

  evalTest({
    name: 'explains a non-trivial algorithm clearly',
    category: 'hero-scenario',
    policy: 'USUALLY_PASSES',
    files: {
      'sort.py': `
def quicksort(arr):
    if len(arr) <= 1:
        return arr
    pivot = arr[len(arr) // 2]
    left = [x for x in arr if x < pivot]
    mid = [x for x in arr if x == pivot]
    right = [x for x in arr if x > pivot]
    return quicksort(left) + mid + quicksort(right)
`.trim(),
    },
    prompt: 'Explain how the quicksort function in sort.py works, including its time complexity.',
    assert: async (run) => {
      await expectJudgeYes(
        run,
        'Does the explanation correctly describe the divide-and-conquer approach of quicksort AND mention its average-case time complexity of O(n log n)?',
      );
    },
  });

});
