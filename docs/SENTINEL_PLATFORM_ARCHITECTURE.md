# Sentinel Platform — System Design Document
## Gateway + Tinker + Agent Architecture

**Version:** 1.0
**Date:** 2026-07-25

---

## Table of Contents

1. [System Overview](#1-system-overview)
2. [Gateway — LLM Router](#2-gateway--llm-router)
3. [Tinker — Training API](#3-tinker--training-api)
4. [Agent — Orchestrator (Existing)](#4-agent--orchestrator-existing)
5. [Shared Platform Layer](#5-shared-platform-layer)
6. [API Specifications](#6-api-specifications)
7. [Data Model](#7-data-model)
8. [Infrastructure & Deployment](#8-infrastructure--deployment)
9. [Security Model](#9-security-model)
10. [Scalability & Performance](#10-scalability--performance)

---

## 1. System Overview

### 1.1 High-Level Architecture

```
┌─────────────────────────────────────────────────────────────────────────┐
│                            EDGE / CDN                                   │
│                    Cloudflare / Fastly / Varnish                        │
└────────────────────────────────┬────────────────────────────────────────┘
                                 │
┌────────────────────────────────▼────────────────────────────────────────┐
│                          LOAD BALANCER                                  │
│                      (HAProxy / Nginx / ALB)                            │
└───────┬──────────────────────┬──────────────────────┬───────────────────┘
        │                      │                      │
┌───────▼──────┐      ┌───────▼──────┐      ┌───────▼──────┐
│  Gateway     │      │  Tinker      │      │  Agent       │
│  Service     │      │  API Service │      │  Proxy       │
│  (axum/Rust) │      │  (FastAPI/   │      │  (axum/Rust) │
│              │      │   Python)    │      │              │
└───────┬──────┘      └───────┬──────┘      └───────┬──────┘
        │                      │                      │
┌───────┴──────────────────────┴──────────────────────┴───────────────────┐
│                          MESSAGE QUEUE                                  │
│                     Redis / RabbitMQ / Kafka                            │
└───────┬──────────────────────┬──────────────────────┬───────────────────┘
        │                      │                      │
┌───────▼──────┐      ┌───────▼──────┐      ┌───────▼──────────────────┐
│  Usage DB    │      │  Training    │      │  Model Registry          │
│  (Postgres)  │      │  Cluster     │      │  (Checkpoint Store +     │
│              │      │  (K8s + GPU) │      │   Metadata)              │
└──────────────┘      └──────────────┘      └──────────────────────────┘
        │                      │                      │
        └──────────────────────┴──────────────────────┘
                               │
                    ┌──────────▼──────────┐
                    │    Object Store     │
                    │   (S3 / R2 / GCS)   │
                    └─────────────────────┘
```

### 1.2 Service Dependency Graph

```
Gateway ────► Postgres (usage, API keys)
Gateway ────► Redis (rate limiter, cache)
Gateway ────► Provider APIs (upstream LLM)

Tinker ────► Postgres (jobs, metadata)
Tinker ────► Redis (job queue, locking)
Tinker ────► K8s GPU Cluster (training pods)
Tinker ────► S3 (checkpoints, datasets)
Tinker ────► Gateway (deploy trained models)

Agent ────► Gateway (LLM inference)
Agent ────► Tinker (optional fine-tuning)
Agent ────► Platform Auth (identity)

Admin Dashboard ────► All services (read/write)
```

### 1.3 Technology Stack

| Component | Technology | Rationale |
|-----------|-----------|-----------|
| **Gateway** | Rust (axum) — reuse `sentinel-proxy` | Performance, existing codebase |
| **Tinker API** | Python (FastAPI) | ML ecosystem (PyTorch, JAX, transformers) |
| **Tinker Training** | Python (PyTorch + torchrun) | GPU training libraries |
| **Agent** | Rust (existing 26 crates) | Already built |
| **Auth Service** | Rust (axum) + SQLx | Performance, JWT handling |
| **Billing Engine** | Rust or Go | High-throughput metering |
| **Admin Dashboard** | React + TypeScript (Vite) | Team expertise, existing frontend patterns |
| **Database** | PostgreSQL 16 | JSONB for flexible schemas, TimescaleDB for time-series |
| **Cache** | Redis 7 | Rate limiting, session cache, job queue |
| **Queue** | Redis Streams / RabbitMQ | Job scheduling, async processing |
| **Object Store** | S3-compatible (MinIO / R2 / AWS S3) | Checkpoints, datasets, logs |
| **Orchestration** | Kubernetes + Helm | GPU scheduling, auto-scaling |
| **Observability** | OpenTelemetry + Grafana + Loki + Tempo | Unified tracing |
| **CI/CD** | GitHub Actions (existing) | Already configured |

---

## 2. Gateway — LLM Router

### 2.1 Component Architecture

```
┌────────────────────────────────────────────────────────────────┐
│                    GATEWAY SERVICE                              │
│                                                                │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────────┐  │
│  │ Router   │  │ Auth     │  │ Rate     │  │ Usage        │  │
│  │ Layer    │  │ Middle   │  │ Limiter  │  │ Tracker      │  │
│  └────┬─────┘  │ ware     │  │          │  │              │  │
│       │        └──────────┘  └──────────┘  └──────────────┘  │
│       │                                                     │
│       ▼                                                     │
│  ┌─────────────────────────────────────────────────────────┐ │
│  │                   REQUEST PIPELINE                       │ │
│  │                                                         │ │
│  │  1. Parse request (model, messages, params)              │ │
│  │  2. Resolve provider from model ID                       │ │
│  │  3. Translate to provider-native format                  │ │
│  │  4. Apply prompt caching (if enabled)                    │ │
│  │  5. Send to provider with fallback logic                 │ │
│  │  6. Translate response back to OpenAI format             │ │
│  │  7. Record usage + cost                                  │ │
│  │  8. Return response to client                            │ │
│  └─────────────────────────────────────────────────────────┘ │
│                                                                │
│  ┌─────────────────────────────────────────────────────────┐ │
│  │              PROVIDER ADAPTERS                           │ │
│  │                                                         │ │
│  │  ┌────────┐ ┌────────┐ ┌────────┐ ┌────────┐ ┌───────┐ │ │
│  │  │OpenAI  │ │Anthropi│ │Google  │ │DeepSeek│ │ 50+   │ │ │
│  │  │Adapter │ │c Adapt.│ │Adapter │ │Adapter │ │ More  │ │ │
│  │  └────────┘ └────────┘ └────────┘ └────────┘ └───────┘ │ │
│  └─────────────────────────────────────────────────────────┘ │
└────────────────────────────────────────────────────────────────┘
```

### 2.2 Model Router Design

```
Input: model = "anthropic/claude-sonnet-4"

Router.parse("anthropic/claude-sonnet-4")
  → provider: Anthropic
  → model_id: claude-sonnet-4
  → deployment: anthropic-api (production)

Router.resolve_endpoint("anthropic-api", "claude-sonnet-4")
  → url: https://api.anthropic.com/v1/messages
  → api_key: <from vault>
  → headers: { anthropic-version: "2023-06-01" }

Router.translate(request, "anthropic")
  → Converts OpenAI-style messages → Anthropic Messages API format
  → Maps system message → system parameter
  → Maps max_tokens, temperature
  → Returns provider-native request body

Router.send_with_fallback(provider_request, fallback_chain)
  → try: POST https://api.anthropic.com/v1/messages
  → catch 5xx/timeout:
    → fallback: fal-ai/anthropic
    → try: POST https://api.fal.ai/anthropic/v1/messages
  → catch again:
    → fallback: openai/gpt-4o
    → translate request to OpenAI format
    → try: POST https://api.openai.com/v1/chat/completions
```

### 2.3 Provider Adapter Interface

```rust
#[async_trait]
pub trait ProviderAdapter: Send + Sync {
    /// The provider's identifier (e.g., "anthropic", "openai")
    fn provider_name(&self) -> &'static str;

    /// The OpenAI-compatible model name patterns this adapter handles
    fn matches_model(&self, model: &str) -> bool;

    /// Translate OpenAI-compatible request to provider-native format
    fn translate_request(&self, request: GatewayRequest) -> Result<ProviderRequest, Error>;

    /// Translate provider-native response back to OpenAI format
    fn translate_response(&self, response: ProviderResponse) -> Result<GatewayResponse, Error>;

    /// Determine the target URL for this model/deployment
    fn resolve_endpoint(&self, model: &str, config: &GatewayConfig) -> Result<Endpoint, Error>;

    /// Send the request with streaming support
    async fn send_stream(
        &self,
        endpoint: Endpoint,
        request: ProviderRequest,
    ) -> Result<BoxStream<'static, Result<Bytes, Error>>, Error>;

    /// Send the request (non-streaming)
    async fn send(
        &self,
        endpoint: Endpoint,
        request: ProviderRequest,
    ) -> Result<ProviderResponse, Error>;
}
```

### 2.4 Provider Translation Layer

Each provider adapter implements a bidirectional translation between OpenAI's chat format and the provider's native format:

| Provider | Input Format | Output Format | Special Handling |
|----------|-------------|---------------|-----------------|
| OpenAI | Native OpenAI | Native OpenAI | Passthrough |
| Anthropic | System → `system` param, messages → `messages` array | `content: [{type: "text", text: ...}]` | Map tool_use content blocks |
| Google Gemini | Messages → `contents` array, system → `system_instruction` | `candidates[0].content` | Map safety settings |
| DeepSeek | Native OpenAI (compatible) | Native OpenAI | FIM prefix/suffix for completions |
| AWS Bedrock | Messages → Converse API | Converse response | AWS SigV4 signing |
| Azure OpenAI | Native OpenAI + deployment ID | Native OpenAI | `api-version` header |
| Together | Native OpenAI | Native OpenAI | Route to correct endpoint |
| Groq | Native OpenAI | Native OpenAI | Faster, smaller models |

### 2.5 Fallback Engine

```rust
pub struct FallbackChain {
    pub primary: ProviderRoute,
    pub alternatives: Vec<ProviderRoute>,
    pub strategy: FallbackStrategy,
}

pub enum FallbackStrategy {
    /// Try primary, then alternatives in order
    Sequential,
    /// Try all in parallel, return first success
    Race,
    /// Try primary, then alternatives after latency threshold
    LatencyAware { threshold_ms: u64 },
}

pub struct FallbackResult {
    pub success: bool,
    pub provider_used: String,
    pub model_used: String,
    pub latency_ms: u64,
    pub attempts: Vec<ProviderAttempt>,
}

pub struct ProviderAttempt {
    pub provider: String,
    pub status: AttemptStatus,  // Success | Failed | Skipped
    pub latency_ms: u64,
    pub error: Option<String>,
}
```

### 2.6 Usage Tracking Pipeline

```
Request received
  │
  ▼
Generate usage_id (ulid)
  │
  ▼
Parse request and record usage_start
  │  ┌───────────────────────┐
  │  │ Redis: pending_usage  │  (track in-flight requests)
  │  └───────────────────────┘
  │
  ▼
Route and execute request
  │
  ▼
Parse response usage stats
  │
  ▼
Calculate cost from model pricing table
  │
  ▼
┌────────────────────────────────────────────┐
│  Batch write to Postgres (every 5s / 1000) │
│                                            │
│  usage_entries:                             │
│    usage_id, api_key_id, user_id,          │
│    provider, model,                        │
│    prompt_tokens, completion_tokens,       │
│    cost_usd, latency_ms,                   │
│    cached, status, timestamp               │
└────────────────────────────────────────────┘
```

### 2.7 Cache Architecture

```
┌──────────┐    ┌──────────────────┐
│ Request  │───▶│ Cache Key        │
│          │    │ = hash(model +    │
│          │    │   messages +      │
│          │    │   params)         │
└──────────┘    └────────┬─────────┘
                         │
                    ┌────▼─────┐
                    │  Redis   │
                    │  Cache   │
                    └────┬─────┘
                         │
                    ┌────▼─────┐
                    │  Hit?    │
                    └────┬─────┘
                    │          │
                  YES         NO
                    │          │
              ┌─────▼──┐  ┌───▼──────┐
              │ Return  │  │ Forward  │
              │ Cached  │  │ to       │
              │ Response│  │ Provider │
              └─────────┘  └───┬──────┘
                               │
                          ┌────▼─────┐
                          │ Store    │
                          │ Response │
                          │ in Cache │
                          └──────────┘
```

**Caching strategy:**
- **Semantic caching**: Same messages + params → cached response
- **TTL**: System prompts: 1h, User messages: 5min, Assistant responses: not cached
- **Cache invalidation**: Explicit `x-sentinel-no-cache: true` header bypasses cache
- **Cache key**: `SHA256(messages_json + model + temperature + max_tokens)`

---

## 3. Tinker — Training API

### 3.1 Component Architecture

```
┌────────────────────────────────────────────────────────────────┐
│                     TINKER API SERVICE                          │
│  (FastAPI + Celery + Ray)                                      │
│                                                                │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────────┐  │
│  │ Training │  │ Job      │  │ Model    │  │ Dataset      │  │
│  │ Session  │  │ Scheduler│  │ Registry │  │ Manager      │  │
│  │ API      │  │          │  │          │  │              │  │
│  └────┬─────┘  └────┬─────┘  └────┬─────┘  └──────┬───────┘  │
│       │              │              │               │          │
│       ▼              ▼              ▼               ▼          │
│  ┌──────────────────────────────────────────────────────────┐ │
│  │                     WORKER POOL                           │ │
│  │                                                          │ │
│  │  ┌─────────────────────┐  ┌───────────────────────────┐  │ │
│  │  │ GPU Worker 1 (H200) │  │ GPU Worker 2 (A100)       │  │ │
│  │  │ torchrun            │  │ torchrun                   │  │ │
│  │  └─────────────────────┘  └───────────────────────────┘  │ │
│  │  ┌─────────────────────┐  ┌───────────────────────────┐  │ │
│  │  │ GPU Worker 3 (H100) │  │ CPU Worker (preprocessing)│  │ │
│  │  │ torchrun            │  │                           │  │ │
│  │  └─────────────────────┘  └───────────────────────────┘  │ │
│  └──────────────────────────────────────────────────────────┘ │
└────────────────────────────────────────────────────────────────┘
```

### 3.2 Training Session Lifecycle

```
Client calls st.Session(model="deepseek-v3.1")
  │
  ▼
POST /v1/tinker/sessions
  → Creates session record in Postgres
  → Status: "initializing"
  → Allocates GPU from pool
  → Loads base model + LoRA adapters
  → Status: "ready"
  │
  ▼
Client calls session.forward_backward(batch)
  │
  ▼
POST /v1/tinker/sessions/{id}/forward_backward
  → Worker loads batch into GPU memory
  → Forward pass (compute loss)
  → Backward pass (accumulate gradients)
  → Returns loss value
  │
  ▼
Client calls session.optim_step()
  │
  ▼
POST /v1/tinker/sessions/{id}/optim_step
  → Apply accumulated gradients to LoRA weights
  → Update optimizer state (AdamW)
  → Step counter +1
  │
  ▼
Client calls session.save_state("checkpoint-100")
  │
  ▼
POST /v1/tinker/sessions/{id}/save_state
  → Serialize LoRA weights + optimizer state
  → Upload to S3 checkpoint bucket
  → Update model registry with checkpoint metadata
  │
  ▼
Client calls session.close()
  │
  ▼
DELETE /v1/tinker/sessions/{id}
  → Release GPU back to pool
  → Status: "completed"
  → Final checkpoint saved
```

### 3.3 Job Queue & Scheduling

```
┌─────────────────────────────────────────────────────────────────────┐
│                         JOB SCHEDULER                               │
│                                                                     │
│  Queue: "tinker-jobs" (Redis Streams)                               │
│                                                                     │
│  ┌──────────────────────────────────────────────────────────┐      │
│  │  Job Envelope                                             │      │
│  │  ┌──────────────────────────────────────────────────────┐ │      │
│  │  │ job_id:     "tkr-20260725-abc123"                    │ │      │
│  │  │ session_id: "sess-xyz"                                │ │      │
│  │  │ user_id:    "user_456"                                │ │      │
│  │  │ operation:  "forward_backward" | "optim_step"        │ │      │
│  │  │           | "sample" | "save_state"                   │ │      │
│  │  │ params:     { batch: [...], batch_size: 8 }          │ │      │
│  │  │ created_at: 2026-07-25T12:00:00Z                     │ │      │
│  │  │ priority:   50 (1-100)                               │ │      │
│  │  └──────────────────────────────────────────────────────┘ │      │
│  └──────────────────────────────────────────────────────────┘      │
│                                                                     │
│  Scheduling Policy:                                                  │
│  - FIFO within same priority                                       │
│  - Priority preemption: higher priority jobs preempt lower          │
│  - Fair-share: no single user >30% of cluster                      │
│  - GPU affinity: same-model jobs prefer same GPU type               │
│  - Idle timeout: release GPU after 15min of inactivity              │
└─────────────────────────────────────────────────────────────────────┘
```

### 3.4 Training Worker Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                     TRAINING WORKER POD                          │
│  (Kubernetes Pod: 1 GPU, N CPU, M Memory)                      │
│                                                                 │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │                  tinker-worker                            │   │
│  │                                                         │   │
│  │  ┌────────────────────┐  ┌──────────────────────────┐   │   │
│  │  │ LoRA Engine        │  │ Model Store              │   │   │
│  │  │ (PyTorch + PEFT)   │  │ (download base model,    │   │   │
│  │  │                    │  │  cache on local SSD)     │   │   │
│  │  └────────────────────┘  └──────────────────────────┘   │   │
│  │                                                         │   │
│  │  ┌────────────────────┐  ┌──────────────────────────┐   │   │
│  │  │ Optimizer          │  │ Checkpoint Manager       │   │   │
│  │  │ (AdamW + schedule) │  │ (async upload to S3)     │   │   │
│  │  └────────────────────┘  └──────────────────────────┘   │   │
│  │                                                         │   │
│  │  ┌──────────────────────────────────────────────────┐   │   │
│  │  │ Metrics Reporter (→ Prometheus + job logs)        │   │   │
│  │  └──────────────────────────────────────────────────┘   │   │
│  └─────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────┘
```

### 3.5 LoRA Training Engine Details

```python
# tinker.core.lora_engine

class LoraEngine:
    def __init__(self, model_id: str, lora_rank: int = 64):
        self.base_model = self._load_base_model(model_id)
        self.peft_config = LoraConfig(
            r=lora_rank,
            lora_alpha=lora_rank * 2,
            target_modules=["q_proj", "v_proj", "k_proj", "o_proj"],
            bias="none",
            task_type="CAUSAL_LM",
        )
        self.model = get_peft_model(self.base_model, self.peft_config)
        self.model.train()
        self.optimizer = AdamW(self.model.parameters(), lr=1e-4)
        self.lora_weights = {}  # Accumulated LoRA deltas

    def forward_backward(self, batch: dict) -> float:
        """Forward pass + backward pass, accumulate gradients."""
        inputs = tokenizer(batch["text"], return_tensors="pt", padding=True)
        inputs = {k: v.to("cuda") for k, v in inputs.items()}

        outputs = self.model(**inputs, labels=inputs["input_ids"])
        loss = outputs.loss
        loss.backward()

        return loss.item()

    def optim_step(self):
        """Apply accumulated gradients to LoRA weights."""
        self.optimizer.step()
        self.optimizer.zero_grad()

    def sample(self, prompts: list[str], max_tokens: int = 256) -> list[str]:
        """Generate tokens from current model."""
        self.model.eval()
        inputs = tokenizer(prompts, return_tensors="pt", padding=True).to("cuda")
        with torch.no_grad():
            outputs = self.model.generate(
                **inputs,
                max_new_tokens=max_tokens,
                do_sample=True,
                temperature=0.8,
            )
        return tokenizer.batch_decode(outputs)

    def save_state(self, tag: str) -> Checkpoint:
        """Save LoRA weights + optimizer state to checkpoint."""
        state = {
            "lora_weights": self.model.state_dict(),
            "optimizer": self.optimizer.state_dict(),
            "step": self.optimizer._step_count,
            "model_id": self.model_id,
            "lora_config": self.peft_config,
        }
        return self._upload_checkpoint(tag, state)
```

---

## 4. Agent — Orchestrator (Existing)

### 4.1 Current Architecture

The Sentinel Agent already exists as 26 Rust crates. For the platform integration, minimal changes are needed:

```
Current:
  Agent ← direct provider API (env var keys)
  Agent → local SQLite/JSON for sessions
  Agent → local cost tracking

Platform Integration:
  Agent → Gateway API (platform API key)
  Agent → Platform Auth Service (identity)
  Agent → Platform Billing (cost attribution)
  Agent optionally → Tinker (deploy fine-tuned models)
```

### 4.2 Integration Changes

| Change | Impact | Effort |
|--------|--------|--------|
| Add Gateway as a `ModelProvider` implementation | Agent can route through Gateway | 2 days |
| Replace env var auth with Platform API key | Unified auth | 3 days |
| Wire usage data to Platform billing | Centralized cost tracking | 2 days |
| Support Tinker-deployed models as providers | Agent uses fine-tuned models | 3 days |
| Platform dashboard for agent sessions | Team collaboration | 2 weeks |

### 4.3 Gateway ModelProvider Adapter

```rust
/// Provider adapter that routes through Sentinel Gateway
pub struct GatewayProvider {
    client: reqwest::Client,
    gateway_url: String,
    api_key: String,
}

#[async_trait]
impl ModelProvider for GatewayProvider {
    fn info(&self) -> ProviderInfo {
        ProviderInfo {
            name: "sentinel-gateway",
            // 400+ models available through Gateway
            models: vec!["*".to_string()],
        }
    }

    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse> {
        let resp = self.client
            .post(format!("{}/v1/chat/completions", self.gateway_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&request)
            .send()
            .await?;

        Ok(resp.json().await?)
    }

    async fn complete_stream(
        &self,
        request: CompletionRequest,
    ) -> Result<BoxStream<'static, Result<StreamChunk>>> {
        let resp = self.client
            .post(format!("{}/v1/chat/completions", self.gateway_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&request)
            .send()
            .await?;

        Ok(Box::pin(StreamChunkStream::new(resp.bytes_stream())))
    }
}
```

---

## 5. Shared Platform Layer

### 5.1 Authentication Service

```
┌────────────────────────────────────────────────────────────────┐
│                     AUTH SERVICE                               │
│                                                               │
│  ┌──────────────┐  ┌──────────────┐  ┌───────────────────┐   │
│  │ API Key      │  │ OAuth        │  │ API Key          │   │
│  │ Generation   │  │ Login        │  │ Validation        │   │
│  └──────┬───────┘  └──────┬───────┘  └────────┬──────────┘   │
│         │                  │                    │              │
│  ┌──────▼──────────────────▼────────────────────▼──────────┐  │
│  │                   KEY STORE                             │  │
│  │  ┌────────────────────────────────────────────────────┐ │  │
│  │  │ api_keys:                                          │ │  │
│  │  │  id            │ ULID (primary key)                 │ │  │
│  │  │  key_hash      │ SHA3-256 of API key                │ │  │
│  │  │  key_prefix    │ First 8 chars (for identification) │ │  │
│  │  │  user_id       │ FK to users                       │ │  │
│  │  │  name          │ "My Dev Key"                      │ │  │
│  │  │  permissions   │ JSON: ["gateway:read", "tinker:*"]│ │  │
│  │  │  rate_limit    │ 1000 req/min                      │ │  │
│  │  │  budget_cents  │ 50000 ($500/mo)                   │ │  │
│  │  │  is_active     │ true                              │ │  │
│  │  │  created_at    │ timestamp                         │ │  │
│  │  │  expires_at    │ nullable                          │ │  │
│  │  └────────────────────────────────────────────────────┘ │  │
│  └──────────────────────────────────────────────────────────┘  │
└────────────────────────────────────────────────────────────────┘
```

### 5.2 Billing Engine

```
┌────────────────────────────────────────────────────────────────┐
│                     BILLING ENGINE                             │
│                                                               │
│  Metering Pipeline:                                            │
│  ┌──────────┐    ┌──────────┐    ┌──────────┐    ┌─────────┐ │
│  │ Usage    │───▶│ Meter    │───▶│ Rate     │───▶│ Invoice │ │
│  │ Events   │    │ Aggreg.  │    │ Engine   │    │ Generator│ │
│  └──────────┘    └──────────┘    └──────────┘    └─────────┘ │
│                                                               │
│  Rate Table (Postgres):                                       │
│  ┌────────────────────────────────────────────────────────┐   │
│  │ rates:                                                  │   │
│  │  id, product ("gateway"|"tinker"|"agent"),             │   │
│  │  dimension ("tokens"|"gpu_hour"|"storage_gb"),         │   │
│  │  tier ("free"|"dev"|"team"|"enterprise"),              │   │
│  │  unit_price_cents (fractional cents),                  │   │
│  │  included_units (free tier allowance)                   │   │
│  └────────────────────────────────────────────────────────┘   │
│                                                               │
│  Invoice Cycle: Monthly                                       │
│  1. Aggregate all usage events for user in billing period     │
│  2. Apply rate table: included_units → free, rest → billed   │
│  3. Generate invoice line items                              │
│  4. Charge saved payment method (Stripe)                     │
│  5. Email invoice PDF                                        │
└────────────────────────────────────────────────────────────────┘
```

### 5.3 Admin Dashboard

```
┌────────────────────────────────────────────────────────────────┐
│                    ADMIN DASHBOARD                             │
│  (React + Vite + Zustand + Recharts)                          │
│                                                               │
│  ┌───────────────────────────────────────────────────────┐    │
│  │  Navigation:                                          │    │
│  │  [Overview] [Gateway] [Tinker] [Agent] [Settings]     │    │
│  └───────────────────────────────────────────────────────┘    │
│                                                               │
│  ┌──────────────────────┐  ┌──────────────────────────────┐  │
│  │  Overview            │  │  Gateway Tab                 │  │
│  │  ┌──────────────┐   │  │  ┌────────────────────────┐  │  │
│  │  │ MRR: $12.4K  │   │  │  │ API Keys              │  │  │
│  │  │ Active Users: │   │  │  │ ┌─────────┬─────────┐ │  │  │
│  │  │ 1,234        │   │  │  │ │ Key     │ Usage   │ │  │  │
│  │  │ Total Reqs:  │   │  │  │ ├─────────┼─────────┤ │  │  │
│  │  │ 8.5M        │   │  │  │ │ sk-s...a │ $234.50│ │  │  │
│  │  └──────────────┘   │  │  │ │ sk-s...b │ $89.20 │ │  │  │
│  │  ┌──────────────┐   │  │  │ └─────────┴─────────┘ │  │  │
│  │  │ 7d Usage     │   │  │  │ [Create Key] [+ New]  │  │  │
│  │  │ [line chart] │   │  │  └────────────────────────┘  │  │
│  │  └──────────────┘   │  │  ┌────────────────────────┐  │  │
│  └──────────────────────┘  │  │ Usage by Model         │  │  │
│                             │  │ [bar chart: top 10]   │  │  │
│  ┌──────────────────────┐  │  └────────────────────────┘  │  │
│  │  Tinker Tab          │  │  ┌────────────────────────┐  │  │
│  │  ┌────────────────┐  │  │  │ Cost by Provider       │  │  │
│  │  │ Active Jobs: 12│  │  │  │ [pie chart]            │  │  │
│  │  │ GPU Utilization│  │  │  └────────────────────────┘  │  │
│  │  │ 78%            │  │  └──────────────────────────────┘  │  │
│  │  │ Avg Queue: 45s │  │                                    │  │
│  │  └────────────────┘  │                                    │  │
│  └──────────────────────┘                                    │  │
└────────────────────────────────────────────────────────────────┘
```

---

## 6. API Specifications

### 6.1 Gateway API

| Endpoint | Method | Description | Auth |
|----------|--------|-------------|------|
| `/v1/chat/completions` | POST | Chat completion (primary) | API key |
| `/v1/chat/completions` | POST + `stream: true` | Streaming chat completion | API key |
| `/v1/models` | GET | List available models | API key (optional) |
| `/v1/embeddings` | POST | Generate embeddings | API key |
| `/v1/audio/transcriptions` | POST | Transcribe audio | API key |
| `/v1/images/generations` | POST | Generate images | API key |
| `/v1/completions` | POST | Legacy completion (deprecated) | API key |
| `/health` | GET | Health check | None |
| `/metrics` | GET | Prometheus metrics | Internal |

### 6.2 Tinker API

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/v1/tinker/models` | GET | List available base models for training |
| `/v1/tinker/sessions` | POST | Create training session |
| `/v1/tinker/sessions/{id}` | GET | Get session status |
| `/v1/tinker/sessions/{id}` | DELETE | Close session, release GPU |
| `/v1/tinker/sessions/{id}/forward_backward` | POST | Forward + backward pass |
| `/v1/tinker/sessions/{id}/optim_step` | POST | Update weights |
| `/v1/tinker/sessions/{id}/sample` | POST | Generate tokens |
| `/v1/tinker/sessions/{id}/save_state` | POST | Save checkpoint |
| `/v1/tinker/checkpoints` | GET | List all checkpoints |
| `/v1/tinker/checkpoints/{id}` | GET | Get checkpoint metadata |
| `/v1/tinker/checkpoints/{id}/download` | GET | Download checkpoint weights |
| `/v1/tinker/datasets` | POST | Upload dataset |
| `/v1/tinker/datasets/{id}` | GET | Get dataset info |
| `/v1/tinker/jobs` | GET | List training jobs |
| `/v1/tinker/jobs/{id}` | GET | Get job status & logs |
| `/v1/tinker/jobs/{id}/cancel` | POST | Cancel running job |

### 6.3 Platform Admin API

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/v1/admin/keys` | GET | List API keys |
| `/v1/admin/keys` | POST | Create API key |
| `/v1/admin/keys/{id}` | DELETE | Revoke API key |
| `/v1/admin/usage` | GET | Get usage report (by key, time range) |
| `/v1/admin/billing/invoice` | GET | Get current invoice |
| `/v1/admin/billing/payment-method` | POST | Update payment method |
| `/v1/admin/users` | GET | List users |
| `/v1/admin/users/{id}` | GET | Get user details |

### 6.4 Rate Limiting Strategy

```
Rate Limit Algorithm: Sliding Window (Redis Sorted Set)

Key: "ratelimit:{api_key_id}:{endpoint_group}"

Window: 60 seconds
Max: configurable per key (default: 1000 req/min)

On each request:
  1. now = timestamp_ms
  2. window_start = now - 60000
  3. MULTI:
     ZREMRANGEBYSCORE key 0 window_start  (clean old entries)
     ZADD key now now                      (add current request)
     ZCARD key                             (count requests in window)
  4. If count > max → 429 Too Many Requests

Headers returned:
  X-RateLimit-Limit: 1000
  X-RateLimit-Remaining: 987
  X-RateLimit-Reset: 1721845260
```

---

## 7. Data Model

### 7.1 Gateway Database Schema

```sql
-- API Keys
CREATE TABLE api_keys (
    id              ULID PRIMARY KEY,
    key_hash        TEXT NOT NULL UNIQUE,      -- SHA3-256(sk-sentinel-...)
    key_prefix      TEXT NOT NULL,              -- "sk-sentinel-a1b2"
    user_id         UUID NOT NULL REFERENCES users(id),
    name            TEXT NOT NULL,
    permissions     JSONB NOT NULL DEFAULT '[]',
    rate_limit      INT NOT NULL DEFAULT 1000,  -- requests per minute
    budget_cents    INT,                         -- monthly budget
    is_active       BOOLEAN NOT NULL DEFAULT true,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at      TIMESTAMPTZ
);

-- Usage Records
CREATE TABLE usage_entries (
    id                ULID PRIMARY KEY,
    api_key_id        ULID NOT NULL REFERENCES api_keys(id),
    user_id           UUID NOT NULL REFERENCES users(id),
    provider          TEXT NOT NULL,              -- "anthropic"
    model             TEXT NOT NULL,              -- "claude-sonnet-4"
    prompt_tokens     INT NOT NULL,
    completion_tokens INT NOT NULL,
    cached_tokens     INT NOT NULL DEFAULT 0,
    cost_usd          NUMERIC(12,6) NOT NULL,    -- fractional cents
    latency_ms        INT NOT NULL,
    status            TEXT NOT NULL,              -- "success" | "error" | "cached"
    request_id        TEXT NOT NULL UNIQUE,       -- idempotency
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Indexes for common queries
CREATE INDEX idx_usage_api_key ON usage_entries(api_key_id, created_at DESC);
CREATE INDEX idx_usage_user ON usage_entries(user_id, created_at DESC);
CREATE INDEX idx_usage_date ON usage_entries(created_at DESC);

-- Aggregate usage by hour (for dashboard)
CREATE MATERIALIZED VIEW hourly_usage AS
SELECT
    date_trunc('hour', created_at) AS hour,
    api_key_id,
    provider,
    model,
    SUM(prompt_tokens) AS total_prompt_tokens,
    SUM(completion_tokens) AS total_completion_tokens,
    SUM(cost_usd) AS total_cost_usd,
    COUNT(*) AS total_requests
FROM usage_entries
WHERE status = 'success'
GROUP BY 1, 2, 3, 4;

-- Rate Limiter
CREATE TABLE rate_limit_config (
    api_key_id   ULID PRIMARY KEY REFERENCES api_keys(id),
    requests_per_minute INT NOT NULL DEFAULT 1000,
    tokens_per_minute   INT,                     -- optional token-level limit
    concurrent_limit    INT DEFAULT 10
);

-- Provider Health
CREATE TABLE provider_health (
    provider    TEXT NOT NULL,
    model       TEXT NOT NULL,
    status      TEXT NOT NULL,                   -- "healthy" | "degraded" | "unavailable"
    latency_p50 INT,
    latency_p95 INT,
    error_rate  NUMERIC(5,4),
    checked_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (provider, model)
);
```

### 7.2 Tinker Database Schema

```sql
-- Training Sessions
CREATE TABLE training_sessions (
    id              ULID PRIMARY KEY,
    user_id         UUID NOT NULL REFERENCES users(id),
    model_id        TEXT NOT NULL,               -- "deepseek-ai/DeepSeek-V3.1"
    status          TEXT NOT NULL DEFAULT 'initializing',
                                                  -- initializing | ready | running | idle | completed | error
    lora_rank       INT NOT NULL DEFAULT 64,
    gpu_type        TEXT NOT NULL,                -- "h200" | "a100-80gb" | "l40s"
    gpu_count       INT NOT NULL DEFAULT 1,
    node_name       TEXT,                         -- K8s node running the pod
    checkpoint_url  TEXT,                         -- S3 URL of latest checkpoint
    total_steps     INT DEFAULT 0,
    current_loss    NUMERIC(10,6),
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    started_at      TIMESTAMPTZ,
    completed_at    TIMESTAMPTZ,
    expires_at      TIMESTAMPTZ                  -- auto-release GPU after this
);

-- Checkpoints
CREATE TABLE checkpoints (
    id              ULID PRIMARY KEY,
    session_id      ULID NOT NULL REFERENCES training_sessions(id),
    user_id         UUID NOT NULL REFERENCES users(id),
    tag             TEXT NOT NULL,                -- "checkpoint-step-100"
    s3_path         TEXT NOT NULL,
    size_bytes      BIGINT NOT NULL,
    step            INT NOT NULL,
    loss            NUMERIC(10,6),
    model_id        TEXT NOT NULL,
    lora_config     JSONB,
    metadata        JSONB,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE(session_id, tag)
);

-- Datasets
CREATE TABLE datasets (
    id              ULID PRIMARY KEY,
    user_id         UUID NOT NULL REFERENCES users(id),
    name            TEXT NOT NULL,
    s3_path         TEXT NOT NULL,
    format          TEXT NOT NULL,                 -- "jsonl" | "parquet" | "csv"
    num_examples    INT,
    size_bytes      BIGINT,
    schema          JSONB,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Model Registry (deployable models)
CREATE TABLE model_registry (
    id              ULID PRIMARY KEY,
    user_id         UUID NOT NULL REFERENCES users(id),
    source_type     TEXT NOT NULL,                 -- "tinker" | "external" | "gateway"
    source_id       TEXT,                          -- checkpoint_id for Tinker models
    model_id        TEXT NOT NULL,                 -- gateway model ID: "tinker/user/model-name"
    display_name    TEXT,
    status          TEXT NOT NULL DEFAULT 'active', -- active | inactive | deprecated
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE(model_id)
);
```

---

## 8. Infrastructure & Deployment

### 8.1 Kubernetes Architecture

```
┌───────────────────────────────────────────────────────────────┐
│                     KUBERNETES CLUSTER                        │
│                                                              │
│  ┌─────────────────┐  ┌─────────────────┐                   │
│  │  Gateway Pods   │  │  Tinker API     │                   │
│  │  (stateless,    │  │  Pods           │                   │
│  │   HPA 2-20)     │  │  (HPA 2-10)     │                   │
│  └─────────────────┘  └─────────────────┘                   │
│                                                              │
│  ┌─────────────────┐  ┌─────────────────┐                   │
│  │  Auth Service   │  │  Admin          │                   │
│  │  Pods           │  │  Dashboard Pods │                   │
│  │  (HPA 2-5)      │  │  (static)       │                   │
│  └─────────────────┘  └─────────────────┘                   │
│                                                              │
│  ┌──────────────────────────────────────────────────────┐   │
│  │  GPU Node Pool (spot + on-demand mix)                │   │
│  │                                                      │   │
│  │  ┌──────────────┐  ┌──────────────┐                 │   │
│  │  │ GPU Node 1   │  │ GPU Node 2   │  ...            │   │
│  │  │ H200:8       │  │ A100:8       │                 │   │
│  │  │ Training Pod │  │ Training Pod │                 │   │
│  │  └──────────────┘  └──────────────┘                 │   │
│  └──────────────────────────────────────────────────────┘   │
│                                                              │
│  ┌──────────────────────────────────────────────────────┐   │
│  │  Infrastructure Pods                                 │   │
│  │  Postgres, Redis, MinIO, Prometheus, Grafana, Loki   │   │
│  └──────────────────────────────────────────────────────┘   │
└───────────────────────────────────────────────────────────────┘
```

### 8.2 Gateway Deployment

```yaml
# gateway-deployment.yaml (conceptual)
apiVersion: apps/v1
kind: Deployment
metadata:
  name: sentinel-gateway
spec:
  replicas: 5
  selector:
    matchLabels:
      app: sentinel-gateway
  template:
    spec:
      containers:
      - name: gateway
        image: sentinel/gateway:latest
        ports:
        - containerPort: 8080
        env:
        - name: DATABASE_URL
          valueFrom:
            secretKeyRef:
              name: platform-db
              key: url
        - name: REDIS_URL
          value: "redis://redis:6379"
        resources:
          requests:
            cpu: 500m
            memory: 512Mi
          limits:
            cpu: 2000m
            memory: 1Gi
        readinessProbe:
          httpGet:
            path: /health
            port: 8080
          initialDelaySeconds: 3
        livenessProbe:
          httpGet:
            path: /health
            port: 8080
          initialDelaySeconds: 10
---
apiVersion: autoscaling/v2
kind: HorizontalPodAutoscaler
metadata:
  name: sentinel-gateway
spec:
  scaleTargetRef:
    apiVersion: apps/v1
    kind: Deployment
    name: sentinel-gateway
  minReplicas: 3
  maxReplicas: 20
  metrics:
  - type: Resource
    resource:
      name: cpu
      target:
        type: Utilization
        averageUtilization: 70
```

### 8.3 GPU Node Requirements

| GPU Type | VRAM | Pod Density | Use Case | On-Demand $/hr | Spot $/hr |
|----------|------|-------------|----------|----------------|-----------|
| H200 | 141GB | 1 training pod | Large model fine-tuning | $8.00 | $2.40 |
| A100-80GB | 80GB | 1-2 pods | Standard fine-tuning | $4.00 | $1.20 |
| H100 | 80GB | 1-2 pods | High-performance training | $12.00 | $3.60 |
| L40S | 48GB | 2-4 pods | Small model, inference | $2.00 | $0.60 |
| L4 | 24GB | 4-8 pods | Lightweight training | $0.60 | $0.18 |

### 8.4 CI/CD Pipeline

```
GitHub Repository
    │
    ├── PR opened ──► PR Checks:
    │                     ├── cargo fmt/clippy (Rust crates)
    │                     ├── ruff check (Python)
    │                     ├── npm tsc (TypeScript)
    │                     ├── cargo test
    │                     ├── pytest
    │                     └── npm test
    │
    ├── Merge to main ──► Build + Push:
    │                        ├── docker build -t sentinel/gateway:commit-sha
    │                        ├── docker build -t sentinel/tinker-api:commit-sha
    │                        ├── docker build -t sentinel/dashboard:commit-sha
    │                        └── docker push to registry
    │
    └── Tag release ──► Deploy to Staging:
                            ├── helm upgrade gateway --values staging.yaml
                            ├── helm upgrade tinker --values staging.yaml
                            └── helm upgrade dashboard --values staging.yaml
                                   │
                                   ▼
                            Manual Promotion
                                   │
                                   ▼
                            Deploy to Production:
                            ├── helm upgrade gateway --values production.yaml
                            ├── helm upgrade tinker --values production.yaml
                            └── helm upgrade dashboard --values production.yaml
```

### 8.5 Database Deployments

| Database | Deployment | Replication | Backup |
|----------|-----------|-------------|--------|
| Postgres (Platform) | Cloud SQL / RDS | 1 primary + 2 read replicas | Daily snapshots + WAL archive |
| Redis (Cache) | ElastiCache / Memorystore | Cluster mode, 3 shards | AOF persistence |
| Redis (Queue) | Self-hosted on K8s | Sentinel replication | N/A (backlog recovery) |
| S3 (Checkpoints) | Cloud Object Store | Cross-region replication | Versioning + lifecycle |

---

## 9. Security Model

### 9.1 Threat Model

| Threat | Impact | Mitigation |
|--------|--------|-----------|
| API key leak | Unauthorized usage, cost | Key rotation, usage alerts, per-key budgets |
| Provider API key leak | Loss of provider access | Vault for secrets, never store in config files |
| Training data exposure | Privacy breach | Encryption at rest, VPC isolation, access logging |
| Model theft | IP loss | Checkpoint encryption, signed URLs with TTL |
| DDoS on Gateway | Service unavailable | Rate limiting, CDN filtering, DDoS protection |
| Prompt injection | Model misbehavior | Input validation, output filtering (Phase 2) |

### 9.2 Encryption

```
In Transit:
  All external APIs: TLS 1.3
  Internal services: mTLS (optional, Phase 2)
  Redis: TLS (recommended)

At Rest:
  Database: AES-256 (Postgres TDE or application-level)
  Checkpoints: AES-256-GCM with per-user keys
  API keys: SHA3-256 hash (irreversible)

Key Management:
  Provider API keys: HashiCorp Vault or AWS KMS
  User data encryption keys: AWS KMS with envelope encryption
  No secrets in environment variables (Vault sidecar)
```

### 9.3 API Key Format

```
Format: sk-sentinel-<version><random>

Example: sk-sentinel-a1b2c3d4e5f6g7h8i9j0k1l2m3n4o5p6q7r8s9t0u1v2w3x4y5z6

Version: a (single char, for future key format migration)
Random: base62 encoded, 40 chars → ~2^237 bits of entropy

Storage: Only SHA3-256 hash stored in database
Validation: On create, return raw key once. On use, hash and match.
```

### 9.4 Access Control

```
Permission Model: Role-Based (RBAC)

Roles:
  ┌──────────────┬────────────────┬─────────────────┬───────────────┐
  │ Permission   │ Admin          │ Developer       │ Read-only     │
  ├──────────────┼────────────────┼─────────────────┼───────────────┤
  │ gateway:*    │ ✓              │ ✓               │               │
  │ gateway:read │ ✓              │ ✓               │ ✓             │
  │ tinker:*     │ ✓              │ ✓               │               │
  │ tinker:read  │ ✓              │ ✓               │ ✓             │
  │ billing:*    │ ✓              │                 │               │
  │ admin:*      │ ✓              │                 │               │
  │ users:*      │ ✓              │                 │               │
  └──────────────┴────────────────┴─────────────────┴───────────────┘
```

---

## 10. Scalability & Performance

### 10.1 Gateway Performance Targets

| Metric | Target | Measurement |
|--------|--------|-------------|
| **P50 latency** (end-to-end) | <500ms | Excluding upstream LLM time |
| **P95 latency** | <2s | Including fallback time |
| **Throughput per node** | 1000 req/s | Single gateway pod |
| **Max connections** | 10000 concurrent | Per pod |
| **Stream TTFB** | <50ms proxy overhead | First byte after upstream |
| **Cache hit rate** | >30% | For repeated system prompts |

### 10.2 Gateway Autoscaling

```
Metrics-based HPA:

CPU > 70% → scale up
req/s > 500 per pod → scale up
Memory > 800Mi → scale up

Cooldown:
  Scale up: 30s
  Scale down: 300s

Max pods: 50 (regional)
Min pods: 3
```

### 10.3 Tinker GPU Scheduling

```
Scheduling Algorithm: Bin-packing with anti-affinity

1. Group jobs by GPU type requirement
2. Within GPU type, pack jobs onto fewest nodes
3. Anti-affinity: same-user jobs prefer different nodes (fault tolerance)
4. Preemption: lower-priority jobs yield to high-priority within 5min notice

Target Utilization:
  GPU: 85% average
  GPU Memory: 75% average
  Node: 90% average

Backend Options:
  Primary: Kubernetes with volcano scheduler (gang scheduling)
  Fallback: AWS ParallelCluster / Slurm (for large multi-node jobs)
```

### 10.4 Data Retention

| Data Type | Retention | Deletion Policy |
|-----------|-----------|-----------------|
| Gateway usage logs | 90 days active, 1 year cold storage | Partition drop |
| Tinker checkpoints | 30 days after last activity | S3 lifecycle |
| Tinker datasets | Until user deletes | Immediate |
| Training logs | 90 days | Log stream expiry |
| API keys | Until revoked | Soft delete (revoke) |
| Invoices | 7 years (legal) | Immutable storage |

---

## Appendix: Key Implementation Files

### Rust (New/Modified)

```
crates/
├── sentinel-gateway/           # NEW: Gateway service
│   ├── src/
│   │   ├── main.rs             # Entry point, axum server
│   │   ├── router.rs           # Model ID → provider resolution
│   │   ├── adapters/           # Provider adapters
│   │   │   ├── mod.rs
│   │   │   ├── openai.rs
│   │   │   ├── anthropic.rs
│   │   │   ├── google.rs
│   │   │   └── ...
│   │   ├── fallback.rs         # Fallback engine
│   │   ├── translate.rs        # Request/response translation
│   │   ├── cache.rs            # Redis cache layer
│   │   ├── auth.rs             # API key validation
│   │   ├── rate_limit.rs       # Rate limiter
│   │   ├── usage.rs            # Usage tracking
│   │   └── health.rs           # Health & metrics
│   └── Cargo.toml
├── sentinel-tinker-client/     # NEW: Rust client for Tinker API
│   ├── src/
│   │   ├── lib.rs
│   │   └── client.rs
│   └── Cargo.toml
├── sentinel-provider/          # MODIFY: Add GatewayProvider adapter
│   └── src/
│       ├── gateway.rs          # NEW: Routes through Gateway
│       └── ...
```

### Python (New)

```
services/
├── tinker-api/                 # NEW: Tinker FastAPI service
│   ├── main.py                 # FastAPI app
│   ├── requirements.txt
│   ├── Dockerfile
│   ├── src/
│   │   ├── api/
│   │   │   ├── sessions.py     # Session CRUD endpoints
│   │   │   ├── checkpoints.py  # Checkpoint endpoints
│   │   │   ├── datasets.py     # Dataset endpoints
│   │   │   └── dashboard.py    # Dashboard data endpoints
│   │   ├── core/
│   │   │   ├── lora_engine.py  # LoRA training engine
│   │   │   ├── scheduler.py    # Job scheduler
│   │   │   └── models.py       # Model registry
│   │   ├── workers/
│   │   │   ├── gpu_worker.py   # GPU training worker
│   │   │   └── preprocessor.py # Dataset preprocessing
│   │   └── db/
│   │       ├── models.py       # SQLAlchemy models
│   │       └── migrations/     # Alembic migrations
│   └── tests/
├── billing-engine/             # NEW: Billing service
│   ├── main.py
│   ├── src/
│   │   ├── meter.py            # Usage metering
│   │   ├── rates.py            # Rate table management
│   │   └── invoice.py          # Invoice generation
│   └── requirements.txt
```

### Frontend (New)

```
frontend/
├── src/
│   ├── pages/
│   │   ├── GatewayDashboard.tsx   # NEW: Gateway usage, keys, models
│   │   ├── TinkerDashboard.tsx    # NEW: Training jobs, sessions
│   │   ├── AgentDashboard.tsx     # MODIFY: Agent session list
│   │   ├── BillingPage.tsx        # NEW: Invoices, payment
│   │   └── SettingsPage.tsx       # NEW: Profile, keys, preferences
│   ├── components/
│   │   ├── UsageChart.tsx         # Shared chart component
│   │   ├── ApiKeyManager.tsx      # API key CRUD UI
│   │   ├── ModelSelector.tsx      # Gateway model picker
│   │   └── TrainingJobCard.tsx    # Tinker job display
│   └── api/
│       ├── gateway.ts             # Gateway API client
│       ├── tinker.ts              # Tinker API client
│       └── platform.ts            # Platform admin API client
```

---

*This document describes the target architecture for the Sentinel Platform. Implementation should follow the phased approach in the PRD, with Gateway as Phase 1, Tinker as Phase 2, and full platform integration in Phase 3.*
