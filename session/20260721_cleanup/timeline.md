# Timeline

## 2026-07-21
- User asked for detailed PRD based on codebase
- Created PRD-v2.md with full system architecture, agent brain, workflows, etc.
- User corrected: it's for ALL tasks, not just platform/SRE. Updated PRD.
- User said "directly don't code, use GitHub Projects" — started stripping cloud/MLOps
- User then said "like Codex CLI, OpenCode, Claude Code" — pivoted back to coding agent
- Created PRD-v3.md — pure terminal coding agent, stripped all cloud/infra/MLOps
- User asked to actually REMOVE all cloud/infra/MLOps/sandbox code from codebase
- Ran comprehensive search to identify all files to delete/edit
- Deleted ~15 directories/files of pure-topic code
- Cleaned Python agent imports (tools/__init__, agent_loop, _agent_helpers)
- Subagent cleaned Rust sentinel-sandbox dependencies (13 files)
- User said STOP — save context as MD for later
