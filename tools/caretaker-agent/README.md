# Caretaker Agent

Cloud Run-based microservices for automated issue triage, PR generation, and repository maintenance. Inspired by Gemini CLI's caretaker-agent pattern, but integrated with Sentinel's Rust agent core.

## Architecture

```
GitHub Webhook / Cron
        │
        ▼
┌─────────────────────┐
│  Ingestion Service  │  Cloud Run (TypeScript)
│  • Receives events   │
│  • Normalizes data   │
│  • Enqueues for triage│
└────────┬────────────┘
         │ (Pub/Sub)
         ▼
┌─────────────────────┐
│   Triage Worker     │  Cloud Run (Python)
│  • Classifies issues │
│  • Routes to labels  │
│  • Estimates effort  │
└────────┬────────────┘
         │
         ▼
┌─────────────────────┐
│  PR Generator       │  Cloud Run (Python)
│  • Generates fixes   │
│  • Creates PRs       │
│  • Validates output  │
└────────┬────────────┘
         │
         ▼
┌─────────────────────┐
│   Egress Service    │  Cloud Run (TypeScript)
│  • Posts comments    │
│  • Applies labels    │
│  • Creates PRs via API│
└─────────────────────┘
```

## Services

### Ingestion Service (`ingestion-service/`)
- **Language**: TypeScript
- **Entry**: `src/server.ts`
- **Function**: Receives GitHub webhook events, normalizes to internal schema, enqueues to Pub/Sub
- **Auth**: GitHub App token verification

### Triage Worker (`triage-worker/`)
- **Language**: Python
- **Entry**: `src/main.py`
- **Function**: Classifies issues by type (bug/feature/docs), routes to appropriate labels, estimates effort from description
- **Integration**: Uses `sentinel-core` via gRPC for complex triage decisions

### Egress Service (`egress-service/`)
- **Language**: TypeScript
- **Entry**: `src/server.ts`
- **Function**: Takes action on GitHub — posts comments, applies labels, creates PRs via Octokit

### PR Generator (`pr-generator/`)
- **Language**: Python
- **Entry**: `workflow/agent_runner.py`
- **Function**: Generates automated PRs for common fix patterns using Sentinel's agent loop

## Deployment

Each service has its own `Dockerfile` and deploys independently to Cloud Run:

```bash
gcloud run deploy sentinel-ingestion   --source tools/caretaker-agent/cloudrun/ingestion-service
gcloud run deploy sentinel-triage      --source tools/caretaker-agent/cloudrun/triage-worker
gcloud run deploy sentinel-egress      --source tools/caretaker-agent/cloudrun/egress-service
gcloud run deploy sentinel-pr-gen      --source tools/caretaker-agent/cloudrun/pr-generator
```

## Local Development

```bash
# Ingestion
cd tools/caretaker-agent/cloudrun/ingestion-service
npm install && npm run dev

# Triage Worker
cd tools/caretaker-agent/cloudrun/triage-worker
pip install -r requirements.txt && python src/main.py

# Egress Service
cd tools/caretaker-agent/cloudrun/egress-service
npm install && npm run dev
```
