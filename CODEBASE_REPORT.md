# Codebase Report – Concise Per‑File Summary

## Root‑level Files

- **`README.md`** – Introduction to the Platform‑Agent project, installation instructions, usage examples, environment variables, and architecture diagram.
- **`LICENSE`** – Apache‑2.0 license text granting permissive use and distribution.
- **`uv.lock`** – Lock file for the `uv` Python package manager, ensuring reproducible dependency versions.
- **`pyproject.toml`** – Poetry‑style Python project configuration: specifies package name, version, dependencies, build system, and scripts.
- **`Cargo.toml`** – Rust crate manifest for the top‑level Rust code (e2e tests, tools). Lists crate name, edition, dependencies, and binary targets.
- **`MODULE.bazel`** – Bazel module declaration defining external repositories and version constraints for the workspace.
- **`RUST_MIGRATION_PLAN.md`** – Project plan outlining the migration of existing components to Rust, key milestones and risk mitigation.
- **`PRD.md`** – Product requirements document describing the goals, use‑cases, and high‑level architecture.
- **`REVIEW.md`** – Review notes and checklist items for the current codebase.
- **`IMPLEMENTATION_GAPS.md`** – List of known missing features or technical debt.
- **`e2e_test.rs`** – End‑to‑end integration test for the Rust components; exercises CLI commands and verifies output.
- **`prompt.txt`** – Default prompt template used by the agent for initializing conversations.
- **`sentinel.example.toml`** – Example configuration file for Sentinel‑AI runtime settings.
- **`CRATES_AUDIT.md`** – Output of `cargo audit`, documenting known Rust security advisories.
- **`package-lock.json`** – npm lock file pinning exact versions of Node dependencies for the frontend.
- **`.gitignore`** – Patterns for files and directories that Git should ignore (e.g., `node_modules/`, `target/`, `*.pyc`).

## Directory: `tools/argument-comment-lint`

- **`src/lib.rs`** – Core library implementing the linting logic: parses Rust source with `syn`, extracts function argument doc comments, and validates format rules.
- **`src/bin/argument-comment-lint.rs`** – Binary entry point: parses CLI flags (`--files`, `--config`) and invokes the library.
- **`run.py`** – Python wrapper that builds the Rust binary (via `cargo build`) and runs it on a set of source files; used in CI pipelines.
- **`run-prebuilt-linter.py`** – Executes a pre‑compiled linter binary, bypassing the build step for faster CI runs.
- **`list-bazel-targets.sh`** – Bash script that lists Bazel targets associated with this tool, feeding them to the linter.
- **`lint_config.json`** – JSON configuration defining lint rules (e.g., required prefix `Args:` for each argument comment).
- **`lint_aspect.bzl`** – Bazel aspect that attaches the linter to Rust targets, causing lint failures on `bazel build`.
- **`Cargo.toml`** – Rust crate manifest for the linter (name `argument-comment-lint`, dependencies on `syn`, `quote`).
- **`BUILD.bazel`** – Bazel build rules for the Rust library and binary, exposing `rust_library` and `rust_binary` targets.

## Directory: `scripts`

- **`install.py`** – Helper script that installs project dependencies (runs `uv sync`, `npm ci`).
- **`format.py`** – Runs code formatters (`ruff format`, `prettier`) across the repo.
- **`check_blob_size.py`** – Checks size of generated binaries/blobs to enforce size limits in CI.
- **`build_package.py`** – Packages the Python distribution (wheel) and uploads to an artifact store.
- **`build_sft.py`** – Builds a “sft” (Super Fine‑Tuned) model artifact; uses `uv run` to invoke the training script.
- **`asciicheck.py`** – Lints source files for non‑ASCII characters to enforce ASCII‑only codebase.
- **`test-remote-env.sh`** – Shell script for testing the remote execution environment (e.g., CI node).
- **`start-codex-exec.sh`** – Starts the Codex execution server used by the agent.
- **`run_tui_with_exec_server.sh`** – Launches a terminal UI session connected to the exec server.
- **`mock_responses_websocket_server.py`** – Simple WebSocket server that returns canned responses for tests.
- **`just_shell.py`** – Utility that runs arbitrary shell commands and captures output.
- **`check-module-bazel-lock.sh`** – Verifies that Bazel module lock files are up‑to‑date.

## Directory: `frontend`

- **`vite.config.ts`** – Vite config for the React/TypeScript UI, defines module aliases, dev server proxy to the backend (`/api` → `http://[::1]:7860`), and build options.
- **`tsconfig.json`** – Strict TypeScript compiler configuration (paths, JSX, target ES2022) used for both the frontend source and tests.
- **`tsconfig.tsbuildinfo`** – Incremental build cache produced by `tsc` to speed up subsequent builds.
- **`index.html`** – HTML entry point that loads the Vite‑generated bundle and sets up the page title and root element.
- **`package.json`** – npm manifest listing dependencies (React, Ink, TypeScript, Vite, etc.), scripts (`dev`, `build`, `cli`), and project metadata.
- **`package-lock.json`** – Exact lockfile for reproducible npm installations.
- **`CONTEXT.md`** – High‑level description of the frontend architecture, entry points, phase machine, component hierarchy, and slash‑command handling.
- **`test-provider.ts`** – Test helper that provides a mock backend provider for unit tests of the UI components.
- **`STATUS.md`** – CI status summary reporting linting, type‑checking, and test results for the frontend.
- **`src/`** – Source directory containing the core UI code:
  - **`src/index.tsx`** – Entry file that sets up debug logging, shims console, and renders the Ink‑based `<App />`.
  - **`src/app.tsx`** – Main React component implementing the phase machine (startup, model selection, main chat UI) and handling events from the backend.
  - **`src/theme.ts`** – Theme definitions (`dark`, `high‑contrast`, `cyber`) and runtime switching via `/theme`.
  - **`src/events/`** – Event emitters:
    - **`mock-emitter.ts`** – Simulated backend event stream for development and testing.
    - **`ipc-emitter.ts`** – Real IPC bridge to the backend process.
  - **`src/components/`** – UI components:
    - **`startup-sequence.tsx`** – Animated ASCII particle field and boot messages shown on launch.
    - **`model-picker.tsx`** – Interactive list for selecting an LLM model (arrow keys, enter, esc).
    - **`chat-view.tsx`** – Scrollable view of assistant and tool messages.
    - **`input-bar.tsx`** – Multiline input with slash‑command autocomplete.
    - **`status-bar.tsx`** – Bottom bar showing model, mode, token usage, turn count, and session ID.
    - **`provider-picker.tsx`**, **`picker-esc.test.tsx`**, **`status-bar.tsx`** – Additional UI helpers and tests.
  - **`src/hooks/`**, **`src/providers/`**, **`src/tools/`**, **`src/events/`** – Supporting utilities and type definitions used throughout the UI.

## Directory: `.devcontainer`

- **`Dockerfile`** – Defines the development container image (Ubuntu base, installs Rust, Python, Node, and tools).
- **`devcontainer.json`** – VS Code dev‑container configuration (extensions, mount points, post‑create commands).
- **`devcontainer.secure.json`** – Secure variant with restricted permissions for CI environments.
- **`post_install.py`** – Runs after container creation to set up the environment (install dependencies, configure git).
- **`post-start.sh`** – Script executed each time the container starts; e.g., launches language servers.
- **`init-firewall.sh`** – Configures firewall rules inside the container for safe network access.

## Directory: `.github/workflows`

- **`ci.yml`** – Main CI pipeline: runs linting, formatting, tests, builds Rust and frontend, and uploads artifacts.
- **`pr-checks.yml`** – Checks that run on pull‑request events (code review lint, unit tests).
- **`main-branch.yml`** – Deploy‑on‑merge workflow for the `main` branch.
- **`claude.yml`** & **`claude-review.yml`** – Custom workflows for Claude‑based agent evaluations.
- **`README.md`** – Documentation for the CI pipeline structure.
- **`dependabot.yml`** – Configures Dependabot to open PRs for version upgrades of dependencies.

## Directory: `third_party`

- **`v8/BUILD.bazel`** and related `BUILD.*.bazel` files – Bazel build definitions for the V8 JavaScript engine and its libc++ components, used by downstream Rust crates that need a JS engine.
- **`wine/BUILD.bazel`** – Bazel rule to fetch and build Wine for Windows compatibility layers.
- **`powershell/BUILD.bazel`** – Bazel rule for PowerShell tooling used in Windows CI steps.
- **`BUILD.bazel`** (top‑level) – Registers third‑party repositories (e.g., V8, Wine) with the workspace.

## Directory: `patches`

Collection of platform‑specific patches applied during the build:
- `rusqlite_windows.patch` – Fixes Windows compilation of the `rusqlite` crate.
- `rules_rust_*.patch` – Adjusts `rules_rust` for various toolchains (process_wrapper, gnullvm, arm64).
- `ring_windows_*.patch` – Patches the `ring` crate for MSVC, Gnullvm, and ARM64.
- `openssl_windows.patch` – Applies Windows‑specific modifications to OpenSSL build.
- `macos_sdk_frameworks.patch` – Adjusts macOS SDK linking for V8.
- `BUILD.bazel` – Central patch aggregation rule for Bazel.

## Directory: `docs`

- **`providers.md`** – Documentation of supported AI model providers and configuration keys.
- **`engineering-plan.md`** – High‑level engineering roadmap and milestones.
- **`codex-impl-plan.md`** – Detailed implementation plan for the Codex agent.

## Directory: `session` (example session logs)

- **`20260707_234202/timeline.md`**, **`session_state.md`**, **`handoff.md`**, **`files.md`** – Human‑readable logs capturing a debugging session: timestamps, state changes, handoff notes, and file diffs.

## Directory: `session_logs`

JSON files (`*.json`) containing raw interaction logs for replay or analysis.

## Directory: `sentinel_ai.egg-info`

Packaging metadata generated by `setuptools` for the `sentinel_ai` Python package (e.g., `PKG-INFO`, `requires.txt`).

## Directory: `tests/unit`

- **`__pycache__/__init__.cpython-312.pyc`** – Compiled bytecode for unit test package initialization (generated at runtime).

## Miscellaneous Files

- **`lacks.md`**, **`aduit.md`** – Likely typo‑named markdown notes (perhaps “audit” and “lacks”).
- **`uv.lock`** – Locks Python dependencies for reproducible environments.
- **`module.bazelrc`** – Bazel runtime configuration (e.g., `build --config=ci`).

---

*This report provides a concise purpose and key content summary for each file and folder in the repository.*
