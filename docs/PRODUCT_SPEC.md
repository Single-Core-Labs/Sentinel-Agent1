# Sentinel AI: Product Specification

**Version:** 2.0  
**Repository:** `Single-Core-Labs/Sentinel-Agent`

---

## 1. Executive Summary & Vision

An autonomous AI coding agent that handles software engineering tasks — from writing features and debugging code to deploying infrastructure and analyzing data. It uses real tools (filesystem, git, shell, cloud, APIs) while keeping a human in the loop before any destructive action.

### The Problem
Engineers spend hours on repetitive or context-heavy work: debugging crashes, writing boilerplate, reviewing PRs, searching logs, or deploying fixes. Existing coding agents are trapped inside the IDE — they can't run shell commands, touch cloud infrastructure, query production logs, browse the web, or spawn sub-agents for parallel research. They also lack safety boundaries for dangerous operations.

### The Solution
A unified AI teammate operating across the full software engineering stack (code, infra, observability, research, data). It works in your terminal, browser, or via Slack. It researches, writes, debugs, deploys, and fixes — with mandatory human approval on anything that mutates production, ensuring a full audit trail.

---

## 2. Target Users
- **Software Engineers**: Need an AI teammate that works beyond the IDE.
- **Platform / DevOps**: Managing infrastructure, Terraform, and deployments.
- **On-call Responders**: Fast root-cause analysis with guarded remediation.
- **Technical Leads**: Slack-visible, approval-gated automation for the team.

## 3. Non-Goals
- Not a general chat product (Sentinel takes actions, it doesn't just answer).
- Not a CI/CD replacement (Sentinel triggers CI; it doesn't replace it).
- Never mutates production or executes dangerous shell commands without explicit human approval.

---

## 4. Core Workflows

### Code & Feature Development
- **Workflow**: Generates boilerplate, writes implementation, creates unit tests, runs builds (`cargo build`, `npm run build`), and iteratively fixes compiler errors until tests pass.
- **Tools Used**: `write_file`, `edit_file`, `run_shell_command`, `grep_search`.

### Debugging & On-Call Remediation
- **Workflow**: Reads stack traces, queries metrics/logs (via MCP tools like Datadog/Grafana), identifies root causes, proposes a code fix, tests it locally, and creates a PR.
- **Tools Used**: `run_shell_command`, `read_file`, `github_pr`.

### Operations & Infrastructure
- **Workflow**: Plans infrastructure changes, modifies `.tf` files, runs `terraform plan`, and awaits user approval before `terraform apply`.
- **Tools Used**: `run_shell_command` (sandboxed), `write_file`.

## 5. Security & Approvals
Sentinel introduces a 3-tier safety system:
1. **Sandboxing**: All shell execution runs inside an `OSJailSandbox` preventing unauthorized network access or out-of-workspace file writes.
2. **Approval Gate**: Commands are categorized. Harmless reads pass automatically; destructive commands (`rm`, `git commit`, API calls) halt the agent loop until the user approves in the CLI or UI.
3. **Doom Loop Prevention**: State machines detect if the agent is stuck in repetitive tool-failure loops and force a human intervention.
