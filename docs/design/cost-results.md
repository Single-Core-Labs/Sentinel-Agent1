# Cost Results (measured)

Run: 2026-08-04 23:33 | Local model: qwen3:8b | LLM pricing: 2 USD/Mtok (input, illustrative)

| Task | Local tokens | LLM tokens | Delta | Est. cost | Local wall | LLM wall |
|---|---|---|---|---|---|---|
| info | 0 | n/a | n/a | n/a | 5833 ms | n/a |
| models | 0 | n/a | n/a | n/a | 5946 ms | n/a |
| backends | 0 | n/a | n/a | n/a | 10375 ms | n/a |
| recommend | 0 | n/a | n/a | n/a | 5663 ms | n/a |
Notes:
- Local path is `sentinel local <model> /<cmd>` (one-shot); zero LLM tokens by construction.
- LLM path is `sentinel ai --prompt "<task>" --yolo`; tokens parsed from the `[sentinel] session summary:` line.
- Rerun: `powershell -ExecutionPolicy Bypass -File scripts/cost-benchmark.ps1`
- ssh task requires -SSHHost <host> (or the SENTINEL_SSH_HOST env var).
