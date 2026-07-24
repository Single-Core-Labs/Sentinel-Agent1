# Agent Notes

## Local Dev Servers

- Agent (CLI): run `uv run python -m agent.main` or install with `uv tool install -e .` then `sentinel ai`.
- Frontend: from `frontend/`, run `npm ci` if dependencies are missing, then `npm run dev`.
- Backend: from `backend/`, run `uv run uvicorn main:app --host ::1 --port 7860`.
- Frontend URL: http://localhost:5173/
- Backend health check: `curl -g http://[::1]:7860/api`
- Frontend proxy health check: `curl http://localhost:5173/api`

Notes:

- Vite proxies `/api` and `/auth` to `http://localhost:7860`.
- If `127.0.0.1:7860` is already owned by another local process, binding the backend to `::1` lets the Vite proxy resolve `localhost` cleanly.
- Prefer `npm ci` over `npm install` for setup, since `npm install` may rewrite `frontend/package-lock.json` metadata depending on npm version.
- Non-local LLM calls use `https://router.sentinel-ai/v1` with the active Sentinel AI user's token. Web sessions and the CLI default to GLM 5.2.
- When asked to start the local server, export the GitHub CLI token first with `export GITHUB_TOKEN="$(gh auth token)"`.

## Development Checks

- Before every commit, run `uv run ruff check .` and `uv run ruff format --check .`.
- If formatting fails, run `uv run ruff format .`, then re-run the Ruff checks before committing.

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


