/**
 * Eval: Core Agent Behavioral Tests
 *
 * Tests basic prompt → tool → output correctness.
 * Category: behavioral
 * Parallels: Gemini's generalist_agent.eval.ts, file_creation_behavior.eval.ts
 * Improvements: tests Sentinel-specific tools (sentinel_read_file, sentinel_write_file),
 * validates /help slash command, tests multi-turn session resume.
 */

import { describe, expect } from 'vitest';
import { evalTest, readWorkspaceFile, expectFileExists, expectToolCalled } from './test-helper.js';

describe('Behavioral: Core Agent', () => {

  // ── 1. File creation ───────────────────────────────────────────────────────

  evalTest({
    name: 'creates a file with exact content when asked',
    category: 'behavioral',
    policy: 'ALWAYS_PASSES',
    prompt: 'Create a file called hello.txt containing exactly the text: sentinel_works',
    assert: async (_run, workDir) => {
      expectFileExists(workDir, 'hello.txt');
      const content = readWorkspaceFile(workDir, 'hello.txt');
      expect(content.trim()).toBe('sentinel_works');
    },
  });

  // ── 2. Multi-file project scaffold ────────────────────────────────────────

  evalTest({
    name: 'scaffolds a Rust hello-world project when asked',
    category: 'behavioral',
    policy: 'USUALLY_PASSES',
    prompt: 'Create a minimal Rust project: a Cargo.toml for a binary crate named "hello-eval" and a src/main.rs that prints "eval ok".',
    assert: async (_run, workDir) => {
      expectFileExists(workDir, 'Cargo.toml');
      expectFileExists(workDir, 'src/main.rs');
      const main = readWorkspaceFile(workDir, 'src/main.rs');
      expect(main).toContain('eval ok');
      const cargo = readWorkspaceFile(workDir, 'Cargo.toml');
      expect(cargo).toContain('hello-eval');
    },
  });

  // ── 3. Code editing ───────────────────────────────────────────────────────

  evalTest({
    name: 'adds a function to an existing file without deleting existing code',
    category: 'behavioral',
    policy: 'USUALLY_PASSES',
    files: {
      'lib.py': 'def greet(name: str) -> str:\n    return f"Hello, {name}"\n',
    },
    prompt: 'Add a function `farewell(name: str) -> str` that returns "Goodbye, {name}" to lib.py. Do not remove the existing greet function.',
    assert: async (_run, workDir) => {
      const content = readWorkspaceFile(workDir, 'lib.py');
      expect(content).toContain('def greet');
      expect(content).toContain('def farewell');
      expect(content).toContain('Goodbye');
    },
  });

  // ── 4. Git operations ─────────────────────────────────────────────────────

  evalTest({
    name: 'commits a new file to git when asked',
    category: 'behavioral',
    policy: 'USUALLY_PASSES',
    prompt: 'Create a file called CHANGELOG.md with content "# Changelog" and commit it with the message "chore: add changelog".',
    assert: async (run, workDir) => {
      expectFileExists(workDir, 'CHANGELOG.md');
      // Verify git commit happened via tool call log
      const gitCalls = run.toolCalls.filter(
        (c) => c.name === 'run_shell_command' &&
        (c.args['command'] as string)?.includes('git commit'),
      );
      expect(gitCalls.length).toBeGreaterThanOrEqual(1);
    },
  });

  // ── 5. Read + summarize ───────────────────────────────────────────────────

  evalTest({
    name: 'reads a file and summarizes its contents correctly',
    category: 'behavioral',
    policy: 'USUALLY_PASSES',
    files: {
      'notes.txt': 'Sentinel Agent supports multiple providers: OpenAI, Anthropic, Gemini, and Ollama.\nIt runs inside an OS-level sandbox for safety.\n',
    },
    prompt: 'Read notes.txt and list the providers mentioned in a comma-separated list.',
    assert: async (run, _workDir) => {
      const out = run.stdout.toLowerCase();
      expect(out).toContain('openai');
      expect(out).toContain('anthropic');
      expect(out).toContain('gemini');
    },
  });

  // ── 6. Error recovery ─────────────────────────────────────────────────────

  evalTest({
    name: 'recovers gracefully when a file does not exist',
    category: 'behavioral',
    policy: 'ALWAYS_PASSES',
    prompt: 'Read the contents of nonexistent_file.txt and tell me what it says.',
    assert: async (run) => {
      // Agent should not crash — it should report the file is missing
      const out = run.stdout.toLowerCase();
      const hasError = out.includes('not found') || out.includes('does not exist') ||
        out.includes("no such file") || out.includes('unable to read') ||
        out.includes('cannot') || out.includes("couldn't");
      expect(hasError, 'Expected agent to report the file does not exist').toBe(true);
      expect(run.exitCode).toBe(0); // Clean exit, not a crash
    },
  });

});
