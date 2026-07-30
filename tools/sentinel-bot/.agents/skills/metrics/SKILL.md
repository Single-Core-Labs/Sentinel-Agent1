# Metrics Collection Skill

Collects repository health metrics via the `gh` CLI.

## Capabilities
- Open PR count, median age, oldest PR
- Open issue count, backlog age distribution
- Actions spend and workflow success rate
- Issue throughput (closed per day)
- Review distribution across contributors
- Time to first response for issues

## Usage
Run from the `metrics/` directory or via `cargo run --bin sentinel-bot-metrics`.

## Output
Writes CSV-formatted results to `history/metrics-<date>.csv` for the Brain to analyze.
