# PRODUCT REQUIREMENTS DOCUMENT — Sentinel Platform
## Unified LLM Gateway + Training API + Agent Orchestrator

**Version:** 1.0
**Date:** 2026-07-25
**Status:** Draft
**Repository:** `Single-Core-Labs/Sentinel-Agent`

---

## Table of Contents

1. [Executive Summary](#1-executive-summary)
2. [Three Products, One Platform](#2-three-products-one-platform)
3. [Target Users](#3-target-users)
4. [Sentinel Gateway — LLM Router (like OpenRouter)](#4-sentinel-gateway--llm-router-like-openrouter)
5. [Sentinel Tinker — Training API (like Thinking Machines Tinker)](#5-sentinel-tinker--training-api-like-thinking-machines-tinker)
6. [Sentinel Agent — AI Engineering Orchestrator (Existing)](#6-sentinel-agent--ai-engineering-orchestrator-existing)
7. [Platform Integration](#7-platform-integration)
8. [Business Model & Pricing](#8-business-model--pricing)
9. [Development Phases](#9-development-phases)
10. [Success Metrics & Risks](#10-success-metrics--risks)

---

## 1. Executive Summary

### One Sentence

A unified AI platform that gives teams a single API to access 400+ LLM models (Sentinel Gateway), a training API to fine-tune open-source models without managing GPU infrastructure (Sentinel Tinker), and an autonomous AI engineering agent to execute complex multi-step tasks (Sentinel Agent).

### The Problem

Teams building AI-powered products face three separate challenges:

| Challenge | Today's Reality |
|-----------|----------------|
| **LLM Access** | Every provider has a different API, auth scheme, billing model. Teams either vendor-lock or build brittle multi-provider integrations. No centralized observability, cost tracking, or fallback. |
| **Model Training** | Fine-tuning open-source models requires managing GPU clusters, installing CUDA/cuDNN, handling distributed training, checkpointing, and scheduling. Researchers spend more time on infra than on research. |
| **AI Engineering** | Coding assistants exist but they live in the IDE — they can't run shell commands, touch cloud infrastructure, query production logs, or spawn sub-agents. No safety gates for production mutations. |

### The Solution — Sentinel Platform

Three integrated products that share authentication, billing, infrastructure, and observability:

```
┌─────────────────────────────────────────────────────────────────────┐
│                      SENTINEL PLATFORM                              │
│                                                                     │
│  ┌──────────────────┐  ┌──────────────────┐  ┌──────────────────┐  │
│  │   GATEWAY        │  │   TINKER         │  │   AGENT          │  │
│  │   (LLM Router)   │  │   (Training API)  │  │   (Orchestrator) │  │
│  │                  │  │                  │  │                  │  │
│  │  One API → 400+  │  │  forward_backward│  │  Plan→Act→Observe│  │
│  │  models          │  │  optim_step      │  │  Tool system     │  │
│  │  Provider        │  │  sample          │  │  Approval gates  │  │
│  │  fallback        │  │  save_state      │  │  Sub-agents      │  │
│  │  Cost tracking   │  │  LoRA fine-tune  │  │  MCP ecosystem   │  │
│  │  Rate limiting   │  │  Job scheduling  │  │  Multi-provider  │  │
│  └────────┬─────────┘  └────────┬─────────┘  └────────┬─────────┘  │
│           │                     │                     │            │
│           ▼                     ▼                     ▼            │
│  ┌──────────────────────────────────────────────────────────────┐  │
│  │              SHARED PLATFORM LAYER                            │  │
│  │  Auth (API keys, OAuth) │ Billing (usage-based, credits)    │  │
│  │  Observability (logs, metrics, tracing) │ Admin dashboard    │  │
│  └──────────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────────┘
```

---

## 2. Three Products, One Platform

### 2.1 Product Comparison

| Dimension | Gateway | Tinker | Agent |
|-----------|---------|--------|-------|
| **What it does** | Routes LLM inference requests | Trains/fine-tunes models | Executes multi-step engineering tasks |
| **Users** | Any developer/application | ML researchers, fine-tuning teams | Platform engineers, DevOps, MLOps |
| **API style** | REST (OpenAI-compatible) | gRPC + REST | CLI / TUI / SDK / LSP |
| **State** | Stateless (caching optional) | Stateful (long-running jobs) | Stateful (sessions) |
| **Infra need** | Edge + regional servers | GPU clusters | Local execution or cloud |
| **Current status** | ~60% built (sentinel-proxy) | 0% built | ~85% built (existing Rust agent) |

### 2.2 How They Work Together

```
Developer needs to fine-tune a model for their agent
         │
         ▼
1. Uses Sentinel Tinker API to fine-tune a model on their data
         │
         ▼
2. Model checkpoint saved to platform storage
         │
         ▼
3. Model registered in Sentinel Gateway as a deployable endpoint
         │
         ▼
4. Sentinel Agent uses the fine-tuned model via Gateway
         │
         ▼
5. All usage tracked, billed, and observable through shared dashboard
```

---

## 3. Target Users

### Primary Personas

| Persona | Pain Point | Uses |
|---------|-----------|-------|
| **Full-stack developer** | Needs one API key for any model | Gateway SDK |
| **ML researcher** | Wants to iterate on training loops without cluster management | Tinker API |
| **Platform engineer** | Needs an AI teammate that works across code, infra, and observability | Agent CLI |
| **Startup CTO** | Wants to build AI features without infra overhead | Gateway + Tinker |
| **Enterprise AI team** | Needs centralized cost control, audit, and model governance | All three + admin dashboard |

### Secondary Personas

| Persona | Need |
|---------|------|
| **Hobbyist developer** | Free tier for Gateway, affordable fine-tuning |
| **AI safety researcher** | Full control over training loop, verifiable checkpoints |
| **Managed service provider** | White-label Gateway for their own customers |
| **University lab** | Subsidized Tinker access for research |

---

## 4. Sentinel Gateway — LLM Router (like OpenRouter)

### 4.1 Overview

A unified API gateway that provides a single OpenAI-compatible endpoint for 400+ models across 70+ providers, with built-in fallback, caching, cost tracking, and rate limiting.

### 4.2 Feature Requirements

#### P0 — Launch Critical

| ID | Feature | Description | Priority |
|----|---------|-------------|----------|
| GW-01 | OpenAI-compatible `/v1/chat/completions` | Single endpoint for all models. Accepts OpenAI SDK format, translates to provider-native format. | P0 |
| GW-02 | Model routing by model ID string | `model: "anthropic/claude-sonnet-4"` → routes to Anthropic. Supports `provider/model` format. | P0 |
| GW-03 | Multi-provider support | OpenAI, Anthropic, Google, DeepSeek, NVIDIA, Moonshot, ZhipuAI, AWS Bedrock, Azure OpenAI, Together, Fireworks, Groq, Perplexity, Replicate | P0 |
| GW-04 | API key authentication | Scoped API keys with permissions (read-only, billing, admin) | P0 |
| GW-05 | Usage tracking & cost calculation | Track prompt tokens, completion tokens, cost per request, per API key, per time period | P0 |
| GW-06 | Basic rate limiting | Per-key, per-IP, per-endpoint rate limits | P0 |
| GW-07 | Provider fallback | Automatic retry with fallback provider on 5xx, timeout, rate limit | P0 |
| GW-08 | Streaming support | SSE streaming through the proxy with minimal overhead | P0 |
| GW-09 | Health check `/health`, `/v1/models` | Service health + list of available models | P0 |
| GW-10 | Request/response logging | Log all requests for audit and debugging | P0 |

#### P1 — High Priority

| ID | Feature | Description |
|----|---------|-------------|
| GW-11 | Prompt caching | Cache frequent system prompts for lower latency and cost |
| GW-12 | Cost budgets per API key | Monthly/ daily spend caps per key |
| GW-13 | Custom model pricing override | Override provider prices with custom markups |
| GW-14 | Geographic routing | Route to nearest/lowest-latency provider region |
| GW-15 | Admin dashboard (web UI) | Usage graphs, cost breakdowns, key management |
| GW-16 | Webhook notifications | Usage alerts, budget threshold warnings via webhook |
| GW-17 | `/v1/audio/transcriptions` | Audio transcription via supported providers |
| GW-18 | `/v1/embeddings` | Embedding generation via supported providers |
| GW-19 | `/v1/images/generations` | Image generation via supported providers |

#### P2 — Nice to Have

| ID | Feature | Description |
|----|---------|-------------|
| GW-20 | Structured output enforcement | JSON schema validation on responses |
| GW-21 | Prompt template management | Store and version system prompts |
| GW-22 | Multi-modal routing | Route image/audio inputs to capable models |
| GW-23 | A/B model comparison | Route same request to multiple models, compare results |
| GW-24 | Custom provider plugin | Bring your own provider via plugin API |
| GW-25 | Cache warming | Pre-populate cache for known prompt patterns |

### 4.3 API Design

```http
POST /v1/chat/completions
Authorization: Bearer sk-sentinel-xxxxxxxx

{
  "model": "anthropic/claude-sonnet-4",
  "messages": [
    {"role": "system", "content": "You are a helpful assistant."},
    {"role": "user", "content": "Hello!"}
  ],
  "max_tokens": 1024,
  "temperature": 0.7
}
```

```http
HTTP/1.1 200 OK

{
  "id": "chatcmpl-sentinel-xxx",
  "object": "chat.completion",
  "created": 1721845200,
  "model": "anthropic/claude-sonnet-4",
  "choices": [...],
  "usage": {
    "prompt_tokens": 25,
    "completion_tokens": 50,
    "total_tokens": 75,
    "cost_usd": 0.0015
  },
  "provider": {
    "name": "anthropic",
    "latency_ms": 842,
    "cached": false
  }
}
```

### 4.4 Model ID Convention

```
<provider>/<model>[:<tag>]

Examples:
  anthropic/claude-sonnet-4
  anthropic/claude-opus-4.8:fal-ai
  openai/gpt-4o
  openai/o3
  google/gemini-2.5-pro
  deepseek/deepseek-v4-pro
  nvidia/nemotron-4-340b-instruct
  moonshot/kimi-k2.7-code
  zhipu/glm-5.2
  openrouter/anthropic/claude-sonnet-4      # Route via OpenRouter as fallback
  local/ollama/llama3.2:7b                  # Self-hosted
```

### 4.5 Provider Fallback Strategy

```
Request comes in with model: "anthropic/claude-sonnet-4"

Primary:     anthropic/claude-sonnet-4          → try first
Fallback 1:  anthropic/claude-sonnet-4:fal-ai   → different provider, same model
Fallback 2:  openai/gpt-4o                      → different provider, equivalent model
Fallback 3:  google/gemini-2.5-pro              → last resort

Rules:
- Fallback only on 5xx, timeout, rate-limit (not 4xx auth errors)
- Configurable fallback chain per model
- Automatic degradation marking for providers
- Sticky sessions: once fallback succeeds, keep using it for the session
```

---

## 5. Sentinel Tinker — Training API (like Thinking Machines Tinker)

### 5.1 Overview

A training API that provides researchers with four primitives (`forward_backward`, `optim_step`, `sample`, `save_state`) to fine-tune open-source models using LoRA, without managing GPU infrastructure.

### 5.2 Feature Requirements

#### P0 — Launch Critical

| ID | Feature | Description | Priority |
|----|---------|-------------|----------|
| TK-01 | `forward_backward` | Forward pass + backward pass on a batch, accumulating gradients | P0 |
| TK-02 | `optim_step` | Update LoRA weights based on accumulated gradients | P0 |
| TK-03 | `sample` | Generate tokens from the model (for evaluation, RL actions, inference) | P0 |
| TK-04 | `save_state` | Persist training state (LoRA weights, optimizer state, step count) to checkpoint storage | P0 |
| TK-05 | LoRA fine-tuning | Parameter-efficient fine-tuning via LoRA adapters | P0 |
| TK-06 | Supported model registry | DeepSeek-V3.1, Kimi-K2.6, Nemotron models, Qwen models, GPT-OSS, Inkling | P0 |
| TK-07 | Job scheduling | Queue training jobs, allocate GPU resources, handle preemption | P0 |
| TK-08 | Checkpoint management | List, download, delete, restore checkpoints | P0 |
| TK-09 | Training logs streaming | Real-time log streaming during training | P0 |
| TK-10 | Dataset upload & management | Upload, validate, version datasets | P0 |

#### P1 — High Priority

| ID | Feature | Description |
|----|---------|-------------|
| TK-11 | DPO/RL support | Reinforcement learning training loops |
| TK-12 | Multi-GPU training | Distributed training across multiple GPUs |
| TK-13 | Evaluation harness | Built-in eval on standard benchmarks during training |
| TK-14 | Training job templates | Pre-built recipes (SFT, DPO, RL, distillation) |
| TK-15 | Weights download | Download LoRA weights as safetensors or GGUF |
| TK-16 | Training dashboard | Web UI to monitor training progress, metrics, logs |

#### P2 — Nice to Have

| ID | Feature | Description |
|----|---------|-------------|
| TK-17 | Full fine-tuning (not just LoRA) | Full weight fine-tuning for supported models |
| TK-18 | Quantization-aware training | Train with QLoRA, AWQ, etc. |
| TK-19 | Continual learning support | Training that preserves prior capabilities (SDFT) |
| TK-20 | Privacy-preserving training | Federated learning style, data never leaves customer VPC |

### 5.3 API Design

```python
import sentinel_tinker as st

# Initialize a training session
session = st.Session(
    model="deepseek-ai/DeepSeek-V3.1",
    lora_rank=64,
    learning_rate=1e-4,
)

# Load dataset
dataset = st.Dataset("my-fine-tuning-data.jsonl")

for epoch in range(3):
    for batch in dataset.batches(batch_size=8):
        # Forward + backward: accumulate gradients
        loss = session.forward_backward(batch)

        # Update weights
        session.optim_step()

        if step % 100 == 0:
            # Sample from current model
            samples = session.sample(
                prompts=["What is 2+2?", "Explain quantum computing"],
                max_tokens=128,
            )

            # Save checkpoint
            session.save_state(f"checkpoint-step-{step}")

    print(f"Epoch {epoch} completed. Loss: {loss}")
```

### 5.4 Supported Models (at launch)

| Model | Size | Architecture | Notes |
|-------|------|-------------|-------|
| Inkling (Thinking Machines) | MoE | Custom MoE | Our own model |
| DeepSeek-V3.1 | 671B MoE | MoE | Open weights |
| Kimi-K2.6 | MoE | MoE | Open weights |
| Nemotron-3-Nano-30B-A3B | 30B MoE | MoE | NVIDIA |
| Nemotron-3-Super-120B-A12B | 120B MoE | MoE | NVIDIA |
| Nemotron-3-Ultra-550B-A55B | 550B MoE | MoE | NVIDIA |
| GPT-OSS-120B | 120B MoE | MoE | OpenAI |
| GPT-OSS-20B | 20B MoE | MoE | OpenAI |
| Qwen3.5-4B | 4B dense | Dense | |
| Qwen3.5-9B | 9B dense | Dense | |
| Qwen3.5-35B-A3B | 35B MoE | MoE | |

### 5.5 Infrastructure Requirements

| Resource | Specification |
|----------|---------------|
| **GPU types** | H200 (141GB), A100-80GB, L40S (48GB), H100 (80GB) |
| **GPU cluster** | Kubernetes with GPU node pools, tolerations, binpacking |
| **Storage** | S3-compatible object store for checkpoints, datasets, logs |
| **Networking** | RDMA/InfiniBand for multi-node training |
| **Scheduling** | Custom job queue with priority, preemption, fair-share |
| **Container runtime** | Docker with NVIDIA container toolkit |

---

## 6. Sentinel Agent — AI Engineering Orchestrator (Existing)

### 6.1 Overview

The existing Sentinel Agent is an autonomous AI engineering orchestrator. It is the third product in the platform — the consumer of Gateway for inference and optionally Tinker for fine-tuned models.

### 6.2 Current State vs. Target

| Capability | Current (v1) | Target (Platform v2) |
|------------|-------------|---------------------|
| **LLM providers** | 7 direct integrations | 400+ via Gateway, plus direct |
| **Auth** | Env vars | Platform API keys + Gateway token |
| **Cost tracking** | Local budget guard | Platform billing integration |
| **Usage analytics** | In-memory stats | Platform-wide dashboard |
| **Model fine-tuning** | Not available | Uses Tinker-trained models |
| **Session persistence** | Local SQLite/JSON | Platform-hosted sessions |
| **Team collaboration** | Single user | Multi-user workspaces |

### 6.3 Agent ↔ Platform Integration Points

```
Gateway Integration:
  Agent → Gateway /v1/chat/completions → provider
  Benefits: unified billing, fallback, observability

Tinker Integration:
  Agent detects task → suggests fine-tuning → user trains via Tinker
  → fine-tuned model deployed on Gateway → Agent uses it

Platform Integration:
  All usage tracked via platform auth → single dashboard
  Agent sessions storable in platform
  Team workspace for sharing agent sessions
```

---

## 7. Platform Integration

### 7.1 Shared Platform Layer

All three products share:

| Service | Technology | Purpose |
|---------|-----------|---------|
| **Auth Service** | JWT + API keys | Unified authentication across all products |
| **Billing Engine** | Usage-based metering | Track credits, invoice, rate-limit |
| **Observability Stack** | OpenTelemetry + Grafana | Logs, metrics, traces for all products |
| **Admin Dashboard** | React + FastAPI | Web UI for managing keys, viewing usage, configuring |
| **Rate Limiter** | Redis-based sliding window | Per-key, per-IP rate limiting |

### 7.2 Authentication Architecture

```
┌──────────────┐     ┌──────────────────┐     ┌───────────────┐
│  Developer   │────▶│  Platform Auth   │────▶│  API Key      │
│  Dashboard   │     │  Service         │     │  Validation   │
└──────────────┘     └──────────────────┘     └───────────────┘
                            │
          ┌─────────────────┼─────────────────┐
          ▼                 ▼                 ▼
   ┌──────────┐      ┌──────────┐      ┌──────────┐
   │ Gateway  │      │ Tinker   │      │ Agent    │
   │ Auth     │      │ Auth     │      │ Auth     │
   └──────────┘      └──────────┘      └──────────┘
```

**Key types:**
- **User API keys** — `sk-sentinel-<random>` — for Gateway and Tinker API access
- **OAuth tokens** — GitHub, Google, HuggingFace OAuth for dashboard
- **Agent tokens** — Short-lived JWTs for agent-to-platform communication

### 7.3 Billing Model

```
Gateway Billing:
  ┌──────────────────────────────────────────────────────┐
  │ Usage = Σ(model_price * tokens) per request           │
  │ Markup: 0% (pass-through) for BYOK, 10% for platform  │
  │ Rate: per-million-tokens pricing                      │
  └──────────────────────────────────────────────────────┘

Tinker Billing:
  ┌──────────────────────────────────────────────────────┐
  │ Compute = GPU_hour_rate * hours + storage ($0.10/GB) │
  │ GPU: H200 $8/hr, A100 $4/hr, L40S $2/hr              │
  │ Storage: $0.10/GB-month for checkpoints               │
  │ Training tokens: $0.50/M tokens (forward + backward)  │
  └──────────────────────────────────────────────────────┘

Agent Billing:
  ┌──────────────────────────────────────────────────────┐
  │ Agent = Gateway usage (if using platform models)      │
  │       + optional sandbox compute                      │
  │ Free: CLI usage (developer's own API keys)            │
  └──────────────────────────────────────────────────────┘
```

### 7.4 Observability Stack

```
┌──────────┐   ┌──────────┐   ┌──────────┐
│ Gateway  │   │ Tinker   │   │ Agent    │
│ Metrics  │   │ Metrics  │   │ Metrics  │
└────┬─────┘   └────┬─────┘   └────┬─────┘
     │               │               │
     └───────────────┼───────────────┘
                     ▼
     ┌─────────────────────────────┐
     │   OpenTelemetry Collector   │
     └────────────┬────────────────┘
                  │
        ┌─────────┴─────────┐
        ▼                   ▼
  ┌──────────┐       ┌──────────┐
  │ Grafana  │       │  Tempo   │
  │ (Metrics)│       │ (Traces) │
  └──────────┘       └──────────┘
        ┌──────────┐
        │  Loki    │
        │ (Logs)   │
        └──────────┘
```

---

## 8. Business Model & Pricing

### 8.1 Pricing Tiers

| Tier | Gateway | Tinker | Agent | Price |
|------|---------|--------|-------|-------|
| **Free** | 100K tokens/mo, 3 models | N/A | CLI-only (BYOK) | $0 |
| **Developer** | 10M tokens/mo, 50+ models | 10 GPU-hours/mo | CLI + Gateway integration | $29/mo |
| **Team** | 100M tokens/mo, 200+ models | 100 GPU-hours/mo | + Team workspace, admin | $199/mo |
| **Enterprise** | Custom | Custom | Custom SLA + VPC deploy | Custom |

### 8.2 Gateway-Specific Pricing

```yaml
pricing_model:
  type: per_token_markup
  free_tier:
    tokens_per_month: 100000
    models: ["openai/gpt-4o-mini", "anthropic/claude-haiku", "google/gemini-2.0-flash"]
  standard:
    markup: 10%  # on top of provider cost
    min_cost_per_request: $0.0001
  byok:
    markup: 0%
    rate_limit: 100 req/min
  enterprise:
    markup: negotiated
    sla: 99.9%
```

### 8.3 Tinker-Specific Pricing

```yaml
pricing_model:
  type: compute_hour + storage + token
  gpu_rates:
    h200: $8.00/hr
    a100_80gb: $4.00/hr
    l40s: $2.00/hr
    h100: $12.00/hr
  storage:
    checkpoints: $0.10/GB-month
    datasets: $0.05/GB-month
  training_tokens:
    forward_backward: $0.50/M tokens
    sample: $0.25/M tokens
```

---

## 9. Development Phases

### Phase 1: Gateway MVP (Months 1-3)

**Goal:** Launch Gateway with core routing, auth, billing.

| Milestone | Deliverable | Depends On |
|-----------|-------------|-----------|
| 1.1 | Enhance sentinel-proxy with smart model routing | Existing proxy code |
| 1.2 | API key auth service + rate limiting | New service |
| 1.3 | Provider fallback engine | 1.1 |
| 1.4 | Usage tracking + cost calculation | 1.2 |
| 1.5 | 50+ provider integrations | 1.1 |
| 1.6 | Basic admin dashboard (key mgmt, usage graphs) | 1.4 |
| 1.7 | Public beta launch | 1.1-1.6 |

### Phase 2: Tinker MVP (Months 2-5)

**Goal:** Launch Tinker with core training primitives and GPU orchestration.

| Milestone | Deliverable | Depends On |
|-----------|-------------|-----------|
| 2.1 | GPU cluster setup + Kubernetes configuration | Infrastructure |
| 2.2 | LoRA training engine (Rust/Python) | New service |
| 2.3 | `forward_backward` / `optim_step` / `sample` / `save_state` | 2.2 |
| 2.4 | Job scheduling + queue management | 2.1 |
| 2.5 | Model registry + checkpoint storage | 2.3 |
| 2.6 | Training dashboard | 2.4 |
| 2.7 | Public beta launch | 2.1-2.6 |

### Phase 3: Platform Integration (Months 3-6)

**Goal:** Unify all three products under shared auth, billing, dashboard.

| Milestone | Deliverable | Depends On |
|-----------|-------------|-----------|
| 3.1 | Unify auth across Gateway, Tinker, Agent | Phase 1 + 2 |
| 3.2 | Unified billing engine | 3.1 |
| 3.3 | OpenTelemetry observability stack | 3.1 |
| 3.4 | Agent uses Gateway as default provider | 3.1 |
| 3.5 | Tinker-deployed models auto-register on Gateway | 3.2 |
| 3.6 | Full admin dashboard (all products) | 3.1-3.5 |
| 3.7 | Public platform launch | 3.1-3.6 |

### Phase 4: Scale & Enterprise (Months 6-12)

**Goal:** Enterprise readiness, scale, advanced features.

| Milestone | Deliverable |
|-----------|-------------|
| 4.1 | VPC deployment option for enterprise |
| 4.2 | SOC2/HIPAA compliance |
| 4.3 | Multi-region Gateway deployment (edge) |
| 4.4 | Custom provider plugin SDK |
| 4.5 | Team workspaces + RBAC |
| 4.6 | Advanced analytics + cost optimization recommendations |

---

## 10. Success Metrics & Risks

### 10.1 Key Metrics

| Metric | Target (12 months) |
|--------|-------------------|
| **Gateway** | 10M+ requests/month, 99.9% uptime, <100ms median latency |
| **Tinker** | 1000+ training jobs/month, <10min avg job start time |
| **Agent** | 100K+ active users, <5min avg task completion |
| **Platform** | $100K+ MRR, <5% churn, NPS > 40 |

### 10.2 Critical Risks

| Risk | Impact | Mitigation |
|------|--------|-----------|
| **GPU availability** | Tinker launch delayed | Multi-cloud strategy, spot instance fallback |
| **Provider API changes** | Gateway breaks | Abstracted provider layer, rapid adaptation |
| **Cost overruns** | Low margins | Real-time cost monitoring, auto-scaling limits |
| **Competition** | OpenRouter adds training, Tinker adds routing | Focus on integration (Agent + Gateway + Tinker) |
| **Security** | API key leak, training data exposure | Key rotation policies, encryption at rest/transit |
| **Compliance** | Enterprise won't adopt | SOC2 certification in Phase 4 |

### 10.3 Open Questions

1. Should Gateway support usage-based billing (per-token) or subscription tiers first?
2. Should Tinker support BYO-GPU (customers bring their own GPU cluster)?
3. Should the Agent remain open-source while Gateway/Tinker are commercial?
4. What is the right free tier limit for each product?
5. Should we partner with GPU providers (AWS, GCP, CoreWeave, Lambda) or build our own cluster?

---

*This document is a living PRD for the Sentinel Platform. It should be updated as requirements evolve and learnings emerge from each development phase.*
