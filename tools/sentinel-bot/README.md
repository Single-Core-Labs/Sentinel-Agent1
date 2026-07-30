# Sentinel Bot (Cognitive Repository)

Cognitive automation for the Sentinel AI repository — dual-layer architecture for proactive repository maintenance.

## Architecture

### System 1: The Pulse (Reflex Layer)
- **Purpose**: High-frequency deterministic maintenance (triage, labeling, stale-bot)
- **Frequency**: 30-min cron (`.github/workflows/sentinel-bot-pulse.yml`)
- **Implementation**: Rust scripts via `cargo run --bin sentinel-bot-pulse`
- **Phases**:
  - **Reflex Execution**: Runs triage and routing scripts in `reflexes/scripts/`
- **Output**: Real-time GitHub actions (labels, comments, closes)

### System 2: The Brain (Reasoning Layer)
- **Purpose**: Strategic analysis, policy refinement, proactive optimization
- **Frequency**: 24-hour cron (`.github/workflows/sentinel-bot-brain.yml`)
- **Implementation**: Agentic phases via `cargo run --bin sentinel-bot-brain`
- **Phases**:
  - **Metrics Collection**: Executes scripts in `metrics/scripts/` for repo health
  - **Phase 1 — Reasoning**: Analyzes metric trends, identifies bottlenecks
  - **Phase 2 — Critique**: Technical validation of proposed changes
  - **Phase 3 — Publish**: Promotes approved changes as PRs

## Directory Structure

```
tools/sentinel-bot/
├── README.md                    # This file
├── ci-policy.toml               # CI permission policy
├── metrics/
│   ├── index.rs                 # Metrics runner (executes all scripts)
│   └── scripts/
│       ├── health.rs            # Repository health overview
│       ├── pr_metrics.rs        # PR latency, throughput, review distribution
│       └── issue_metrics.rs     # Issue backlog age, triage rate
├── brain/
│   ├── scheduled.md             # 24-hour strategic analysis prompt
│   └── interactive.md           # Issue/PR response prompt
├── reflexes/
│   ├── mod.rs                   # Reflex runner
│   └── scripts/
│       ├── triage.rs            # Auto-label new issues
│       ├── stale.rs             # Mark stale issues/PRs
│       └── welcome.rs           # Welcome new contributors
├── history/                     # Time-series metrics artifacts
├── .agents/
│   ├── skills/
│   │   ├── prs/SKILL.md         # PR management skill
│   │   ├── memory/SKILL.md      # Persistent memory skill
│   │   ├── metrics/SKILL.md     # Metrics collection skill
│   │   └── critique/SKILL.md    # Technical review skill
│   └── agents/
│       └── WORKER.md            # Worker subagent definition
└── lessons-learned.md           # Structured bot memory
```

## Usage

### Local Metrics Collection

```powershell
cargo run --bin sentinel-bot-metrics
```

### CI Integration

Add to `.github/workflows/`:

```yaml
# .github/workflows/sentinel-bot-pulse.yml (30-min cron)
name: Sentinel Bot Pulse
on:
  schedule: [{ cron: '*/30 * * * *' }]
  workflow_dispatch:
jobs:
  pulse:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - run: cargo run --bin sentinel-bot-pulse
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
```

```yaml
# .github/workflows/sentinel-bot-brain.yml (daily cron)
name: Sentinel Bot Brain
on:
  schedule: [{ cron: '0 6 * * *' }]
  workflow_dispatch:
jobs:
  brain:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - run: cargo run --bin sentinel-bot-brain
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
```

## Security

- **Zero-Trust**: All GitHub data is treated as untrusted
- **Read-Only Reasoning**: Bot cannot push code or post comments directly — only stages file changes via `git add`
- **Credential Protection**: Never prints, logs, or commits secrets
