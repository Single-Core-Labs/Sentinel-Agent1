# Cost Results (measured)

Run: 2026-08-07 20:26 | Local model: qwen3:8b | LLM models: openrouter/openai/gpt-oss-20b:free | Baseline: openrouter/openai/gpt-4o-mini | Pricing: 2 USD/Mtok (illustrative)

| Task | Path | Tokens | Status | Est. cost | vs baseline | Wall |
|---|---|---|---|---|---|---|
| info | local | 0 | pass | $0.0000 | $n/a | 7306 ms |
| info | openrouter/openai/gpt-oss-20b:free | error | fail | $n/a | $n/a | 79673 ms |
Notes:
- Local path is `sentinel local <model> /<cmd>` (one-shot); zero LLM tokens by construction.
- LLM path is `sentinel ai --yolo --prompt "<task>"` (sandboxed, `SENTINEL_SANDBOX=1`); tokens parsed from the `[sentinel] session summary:` line.
- "vs baseline" = baseline $ - model $ (0 = as cheap as baseline). Baseline: one paid model run per task.
- `status` = pass/fail — the output is validated per task (e.g. `info` must mention OS + cores + RAM); failing runs are excluded from the dashboard optimizer.
- Dashboard: `scripts/run-bench-lab.ps1` (serves charts at http://localhost:PORT/cost-lab/).
- Rerun: `powershell -ExecutionPolicy Bypass -File scripts/cost-benchmark.ps1`
- ssh task requires -SSHHost <host> (or the SENTINEL_SSH_HOST env var).
