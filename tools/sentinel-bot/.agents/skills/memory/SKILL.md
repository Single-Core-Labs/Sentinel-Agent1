# Memory Skill

Persistent state management for the Sentinel Bot via `lessons-learned.md`.

## Capabilities
- Read `lessons-learned.md` at start to sync task ledger and decision log
- Report findings at end for the Orchestrator to record
- Track: Task Ledger, Hypothesis Ledger, Decision Log

## Usage
Load at START and END of every brain phase.

## Constraints
- Read-only for worker agents — report findings, do not update the file
- Only the Orchestrator (Brain) writes to `lessons-learned.md`
