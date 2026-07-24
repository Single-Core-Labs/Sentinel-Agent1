# Sentinel LLM Gateway Architecture (Internal OpenRouter)

## Overview
Currently, the Sentinel Agent connects directly to various model providers (OpenAI, Anthropic, Google, etc.). The goal of this project is to build an internal **LLM Gateway (our own OpenRouter)**. The Sentinel Agent will route all of its model inference requests through this single, unified endpoint. 

## Core Objectives (Why are we building this?)
As requested by Manav, building our own endpoint allows us to:
1. **Observe Tokens:** Centralize token counting, cost tracking, and rate limiting across all underlying providers.
2. **Monitor Agent Performance:** Log every prompt and response, track latency, and observe the agent's behavior in real-time for debugging and analytics.
3. **Unified API:** The agent only needs to know how to talk to *one* API (ours), and the Gateway handles the complex routing to external providers.

---

## High-Level Architecture

```mermaid
flowchart LR
    A[Sentinel Agent] -->|Unified API Request\n(e.g., POST /v1/chat/completions)| B(Sentinel LLM Gateway)
    
    subgraph Gateway [Sentinel LLM Gateway]
        B --> C{Authentication & Rate Limiting}
        C --> D[Token Counter / Cost Estimator]
        D --> E[Provider Router]
        E --> F[(Database: Logs, Telemetry, Costs)]
    end
    
    E -->|Route to OpenAI| G[OpenAI API]
    E -->|Route to Anthropic| H[Anthropic API]
    E -->|Route to Google| I[Google Gemini API]
    
    G --> B
    H --> B
    I --> B
    B -->|Unified Response| A
```

## System Components

### 1. The Gateway Server (The "Clone")
A lightweight, high-performance web server (e.g., built in Rust via `axum` or Python via `FastAPI`) that exposes an OpenAI-compatible REST API:
- `POST /v1/chat/completions`

### 2. Request Router & Translation Layer
When the gateway receives a request, it looks at the requested `model` string (e.g., `anthropic/claude-3-5-sonnet`):
- It translates the standard request format into the specific format required by the target provider.
- It injects the company's internal API keys (keeping keys completely hidden from the agent/client side).

### 3. Observability & Telemetry Engine
- **Token Tracking:** Intercepts the response to read the `usage` statistics (prompt tokens, completion tokens) and logs them against the specific agent session or user.
- **Latency Monitoring:** Times how long the external provider took to generate the response (Time to First Token, Total Request Time).
- **Behavior Logging:** Asynchronously saves the prompt and the completion to a database so developers can review exactly what the agent is doing and how it is performing.

## Deployment Strategy
1. **Phase 1 (Local/Staging):** Build the Gateway as a lightweight proxy service running locally alongside the agent (`localhost:8000`) for development.
2. **Phase 2 (Production):** Deploy the Gateway to a scalable cloud environment (e.g., AWS ECS, Vercel, or Cloudflare Workers) so all deployed agents point to `https://gateway.sentinel-ai.com/v1`.

## Next Steps for Implementation
1. **Approve Architecture:** Review this document to ensure it aligns with the vision of observing token usage and agent performance.
2. **Setup Proxy Project:** Initialize a new directory/crate for the Gateway server.
3. **Implement Core Endpoint:** Build the `/v1/chat/completions` pass-through endpoint.
4. **Add Telemetry:** Integrate the token counting and database logging middleware.
