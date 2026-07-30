# Sentinel AI — Tools

Automation and bot infrastructure for the Sentinel AI repository. Patterned after Google Gemini CLI's dual-layer cognitive architecture, but improved with Rust-native metrics, tighter integration with `sentinel-core`, and cross-platform support.

## Structure

```
tools/
├── sentinel-bot/             # GitHub bot — dual-layer cognitive automation
│   ├── README.md             # Bot architecture & usage
│   ├── ci-policy.toml        # CI permission rules
│   ├── metrics/              # Repository health metrics collection
│   ├── brain/                # Strategic reasoning prompts
│   ├── reflexes/             # Deterministic maintenance scripts
│   ├── history/              # Time-series metric artifacts
│   └── .agents/              # Agent skills & subagent definitions
│       ├── skills/           # prs, memory, metrics, critique
│       └── agents/           # worker subagent
├── caretaker-agent/          # Cloud Run microservice automation
│   ├── README.md             # Service architecture & deployment
│   └── cloudrun/
│       ├── ingestion-service/  # GitHub webhook receiver
│       ├── triage-worker/      # Issue classification
│       ├── egress-service/     # GitHub API actions
│       └── pr-generator/       # Automated fix PRs
├── argument-comment-lint/    # Rust lint tool (existing)
└── README.md                 # This file
```

## Key Improvements Over Gemini CLI Bot

| Aspect | Gemini CLI Bot | Sentinel Bot |
|---|---|---|
| Metrics scripts | TypeScript | Rust (`sentinel-bot-metrics`) |
| Agent integration | Generic skills | `sentinel-core` gRPC integration |
| CI policy | TOML | Same format, extended |
| Brain phases | 3 phases | 3 phases + zero-trust security |
| Services | TypeScript + Python | Same, but wired to `sentinel-core` |
| Cross-platform | Linux only | Windows + Linux + macOS |

## Quick Start

```bash
# Collect repository metrics
cargo run --bin sentinel-bot-metrics

# Run pulse (reflex layer)
cargo run --bin sentinel-bot-pulse

# Triage an issue locally
cd caretaker-agent/cloudrun/triage-worker
python src/main.py < test_event.json
```
