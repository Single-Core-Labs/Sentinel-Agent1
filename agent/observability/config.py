"""Observability configuration."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Literal


@dataclass
class ObservabilityConfig:
    """OpenTelemetry observability settings for the agent.

    All settings can be overridden via environment variables
    (prefixed with ``PLATFORM_AGENT_TELEMETRY_``).
    """

    enabled: bool = False
    """Master toggle for all observability signals."""

    traces_enabled: bool = True
    """Enable detailed trace spans (LLM calls, tool calls, agent turns)."""

    service_name: str = "platform-agent"
    """Service name for OTel resource attributes."""

    otlp_endpoint: str = "http://localhost:4317"
    """OTLP collector endpoint (gRPC or HTTP)."""

    otlp_protocol: Literal["grpc", "http"] = "grpc"
    """OTLP transport protocol."""

    outfile: str | None = None
    """If set, write OTel JSON to this file instead of sending to collector."""

    log_prompts: bool = True
    """Include prompt/response text in trace attributes (may contain PII)."""

    sampling_ratio: float = 1.0
    """Fraction of traces to sample (0.0–1.0). 1.0 = sample everything."""

    # Metric export interval
    metric_export_interval_ms: int = 30_000

    # Batch span processor config
    span_export_interval_ms: int = 5_000
    span_export_max_batch_size: int = 512
