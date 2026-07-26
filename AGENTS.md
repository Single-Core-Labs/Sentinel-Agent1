# Agent Notes

## Running the Agent

- **One command:** `cargo run -- ai` — builds and launches the Rust-native interactive agent.
- Or build once: `cargo install --path crates/sentinel-cli` then `sentinel ai`.
- Vite frontend (optional): from `frontend/`, run `npm ci` then `npm run dev`.

## Configuration

- Non-local LLM calls use `https://router.sentinel-ai/v1` with the active Sentinel AI user's token. Web sessions and the CLI default to GLM 5.2.

## Development Checks

- Before every commit, run `cargo check` to verify Rust compilation.

## Git Workflow

- Before creating any new branch or worktree, switch to `main` and pull the latest changes.

## GitHub CLI

- Always use the `gh` CLI for GitHub operations such as opening, editing, inspecting, or commenting on PRs and issues.
- For multiline PR descriptions, prefer `gh pr edit <number> --body-file <file>` over inline `--body` so shell quoting, `$` env-var names, backticks, and newlines are preserved correctly.
- If `gh` reports an invalid token or auth failure, retry the command with `GH_TOKEN` and `GITHUB_TOKEN` unset, for example `env -u GH_TOKEN -u GITHUB_TOKEN gh pr create ...`, so `gh` can use the stored login token instead of a stale environment token.
- In Codex, sandboxed `gh` auth checks can report a valid keyring login as invalid when GitHub network access is restricted. Before telling the user to re-authenticate, retry with both env tokens unset and GitHub network access enabled.

## GitHub PRs

- Open code changes as GitHub PRs first. Do not push code changes directly to the Sentinel AI Space deployment branch or Space remote before the PR has been opened, reviewed, and merged, unless the user explicitly asks to bypass the PR flow.
- After implementing a plan, run the required checks, commit the changes, open a GitHub PR, then start the backend and frontend local dev servers for testing.


