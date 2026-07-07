"""OTel provider initialisation — TracerProvider, MeterProvider, exporters."""

from __future__ import annotations

import json
import logging
import os
from datetime import datetime, timezone
from pathlib import Path
from opentelemetry import metrics, trace
from opentelemetry.exporter.otlp.proto.grpc.metric_exporter import (
    OTLPMetricExporter as GrpcMetricExporter,
)
from opentelemetry.exporter.otlp.proto.grpc.trace_exporter import (
    OTLPSpanExporter as GrpcSpanExporter,
)
from opentelemetry.exporter.otlp.proto.http.metric_exporter import (
    OTLPMetricExporter as HttpMetricExporter,
)
from opentelemetry.exporter.otlp.proto.http.trace_exporter import (
    OTLPSpanExporter as HttpSpanExporter,
)
from opentelemetry.sdk.metrics import MeterProvider
from opentelemetry.sdk.metrics.export import (
    MetricExporter,
    PeriodicExportingMetricReader,
)
from opentelemetry.sdk.resources import Resource
from opentelemetry.sdk.trace import TracerProvider, sampling
from opentelemetry.sdk.trace.export import (
    BatchSpanProcessor,
    SimpleSpanProcessor,
    SpanExporter,
)
from opentelemetry.semconv.resource import ResourceAttributes

from agent.observability.config import ObservabilityConfig

logger = logging.getLogger(__name__)

# Workaround for OTel Python SDK 1.43.0 / API version gap.
# SDK expects `TraceFlags.RANDOM_TRACE_ID` which is missing in API 1.43.0.
try:
    from opentelemetry.trace import TraceFlags as _OTelTraceFlags

    _OTelTraceFlags.RANDOM_TRACE_ID  # noqa: B018 — test existence
except AttributeError:
    _OTelTraceFlags.RANDOM_TRACE_ID = 1  # 0x01 = trace flags bit

# ---------------------------------------------------------------------------
# Module-level singletons
# ---------------------------------------------------------------------------

_tracer_provider: TracerProvider | None = None
_meter_provider: MeterProvider | None = None
_tracer: trace.Tracer | None = None
_meter: metrics.Meter | None = None
_initialized = False

# Prefix for env-var overrides of ObservabilityConfig fields.
_ENV_PREFIX = "PLATFORM_AGENT_TELEMETRY_"

# ---------------------------------------------------------------------------
# Public API
# ---------------------------------------------------------------------------


def init_observability(config: ObservabilityConfig) -> None:
    """Initialise global OTel providers and exporters.

    Idempotent — calling twice is a no-op.
    """
    global _tracer_provider, _meter_provider, _tracer, _meter, _initialized

    if _initialized:
        return

    merged = _apply_env_overrides(config)
    if not merged.enabled:
        logger.info("Observability disabled")
        _initialized = True
        return

    resource = Resource.create(
        {
            ResourceAttributes.SERVICE_NAME: merged.service_name,
            ResourceAttributes.SERVICE_VERSION: _get_version(),
            "telemetry.sdk.name": "opentelemetry",
            "telemetry.sdk.language": "python",
        }
    )

    # ── Trace provider ────────────────────────────────────────────────
    if merged.traces_enabled:
        sampler = _build_sampler(merged.sampling_ratio)
        _tracer_provider = TracerProvider(resource=resource, sampler=sampler)

        if merged.outfile:
            exporter = _FileSpanExporter(Path(merged.outfile))
            _tracer_provider.add_span_processor(SimpleSpanProcessor(exporter))
        else:
            exporter = _build_span_exporter(merged)
            _tracer_provider.add_span_processor(
                BatchSpanProcessor(
                    exporter,
                    schedule_delay_millis=merged.span_export_interval_ms,
                    max_export_batch_size=merged.span_export_max_batch_size,
                )
            )

        trace.set_tracer_provider(_tracer_provider)
        _tracer = trace.get_tracer(merged.service_name, _get_version())
        logger.info(
            "OTel tracing enabled — endpoint=%s proto=%s",
            merged.otlp_endpoint,
            merged.otlp_protocol,
        )
    else:
        _tracer_provider = None
        _tracer = None

    # ── Meter provider ────────────────────────────────────────────────
    metric_exporter = _build_metric_exporter(merged)
    _meter_provider = MeterProvider(
        resource=resource,
        metric_readers=[
            PeriodicExportingMetricReader(
                metric_exporter,
                export_interval_millis=merged.metric_export_interval_ms,
            )
        ],
    )
    metrics.set_meter_provider(_meter_provider)
    _meter = metrics.get_meter(merged.service_name, _get_version())

    _initialized = True
    logger.info(
        "Observability initialised — service=%s traces=%s",
        merged.service_name,
        merged.traces_enabled,
    )


def get_tracer() -> trace.Tracer | None:
    """Return the global tracer, or None if observability is disabled."""
    return _tracer


def get_meter() -> metrics.Meter | None:
    """Return the global meter, or None if observability is disabled."""
    return _meter


def is_observability_enabled() -> bool:
    return _initialized and _tracer_provider is not None


def shutdown_observability() -> None:
    """Flush and shut down all providers."""
    global _tracer_provider, _meter_provider, _tracer, _meter, _initialized

    if _tracer_provider:
        try:
            _tracer_provider.shutdown()
        except Exception:
            pass
        _tracer_provider = None
        _tracer = None

    if _meter_provider:
        try:
            _meter_provider.shutdown()
        except Exception:
            pass
        _meter_provider = None

    _initialized = False
    logger.info("Observability shut down")


# ---------------------------------------------------------------------------
# Internal helpers
# ---------------------------------------------------------------------------


def _apply_env_overrides(config: ObservabilityConfig) -> ObservabilityConfig:
    """Override config fields from environment variables."""
    overrides = {}
    for field_name in ("enabled", "traces_enabled", "log_prompts"):
        env_key = _ENV_PREFIX + field_name.upper()
        val = os.environ.get(env_key)
        if val is not None:
            overrides[field_name] = val.lower() in ("1", "true", "yes")

    for field_name in ("otlp_endpoint", "otlp_protocol", "outfile", "service_name"):
        env_key = _ENV_PREFIX + field_name.upper()
        val = os.environ.get(env_key)
        if val is not None:
            overrides[field_name] = val

    env_sampling = os.environ.get(_ENV_PREFIX + "SAMPLING_RATIO")
    if env_sampling is not None:
        try:
            overrides["sampling_ratio"] = float(env_sampling)
        except ValueError:
            pass

    if not overrides:
        return config

    merged = ObservabilityConfig(
        **{**{f.name: getattr(config, f.name) for f in config.__dataclass_fields__.values()}, **overrides}
    )
    return merged


def _get_version() -> str:
    """Read version from package metadata or fall back to '0.0.0'."""
    try:
        from importlib.metadata import version

        return version("platform-agent")
    except Exception:
        return "0.0.0"


def _build_sampler(ratio: float) -> sampling.Sampler:
    if ratio >= 1.0:
        return sampling.ALWAYS_ON
    if ratio <= 0.0:
        return sampling.ALWAYS_OFF
    return sampling.TraceIdRatioBased(ratio)


def _build_span_exporter(config: ObservabilityConfig) -> SpanExporter:
    if config.otlp_protocol == "http":
        endpoint = config.otlp_endpoint.rstrip("/") + "/v1/traces"
        return HttpSpanExporter(endpoint=endpoint)
    return GrpcSpanExporter(endpoint=config.otlp_endpoint)


def _build_metric_exporter(config: ObservabilityConfig) -> MetricExporter:
    if config.outfile:
        return _FileMetricExporter(Path(config.outfile))
    if config.otlp_protocol == "http":
        endpoint = config.otlp_endpoint.rstrip("/") + "/v1/metrics"
        return HttpMetricExporter(endpoint=endpoint)
    return GrpcMetricExporter(endpoint=config.otlp_endpoint)


# ---------------------------------------------------------------------------
# File-based exporters (JSON-lines format for local dev)
# ---------------------------------------------------------------------------


class _FileSpanExporter(SpanExporter):
    """Write spans as JSON lines to a file."""

    def __init__(self, path: Path) -> None:
        self.path = path
        path.parent.mkdir(parents=True, exist_ok=True)

    def export(self, spans, timeout_millis=30000):
        try:
            lines = []
            for span in spans:
                record = {
                    "timestamp": datetime.now(timezone.utc).isoformat(),
                    "type": "span",
                    "trace_id": format_trace_id(span.get_span_context().trace_id),
                    "span_id": format_span_id(span.get_span_context().span_id),
                    "name": span.name,
                    "kind": str(span.kind),
                    "status": span.status.status_code.name if span.status else "UNSET",
                    "attributes": dict(span.attributes) if span.attributes else {},
                    "start_time": span.start_time,
                    "end_time": span.end_time,
                }
                lines.append(json.dumps(record, default=str))
            with open(self.path, "a", encoding="utf-8") as f:
                f.write("\n".join(lines) + "\n")
        except Exception:
            pass
        return None  # Success

    def shutdown(self, timeout_millis=30000):
        pass

    def force_flush(self, timeout_millis=30000):
        pass


class _FileMetricExporter(MetricExporter):
    """Write metrics as JSON lines to a file."""

    _preferred_temporality = {}
    _preferred_aggregation = {}

    def __init__(self, path: Path) -> None:
        self.path = path
        path.parent.mkdir(parents=True, exist_ok=True)

    def export(self, metrics_data, timeout_millis=30000):
        try:
            lines = []
            for metric in metrics_data:
                record = {
                    "timestamp": datetime.now(timezone.utc).isoformat(),
                    "type": "metric",
                    "name": metric.name,
                    "description": metric.description,
                    "unit": metric.unit,
                    "data": str(metric),
                }
                lines.append(json.dumps(record, default=str))
            with open(self.path, "a", encoding="utf-8") as f:
                f.write("\n".join(lines) + "\n")
        except Exception:
            pass
        return None

    def shutdown(self, timeout_millis=30000):
        pass

    def force_flush(self, timeout_millis=30000):
        pass


def format_trace_id(trace_id: int) -> str:
    return f"{trace_id:032x}"


def format_span_id(span_id: int) -> str:
    return f"{span_id:016x}"
