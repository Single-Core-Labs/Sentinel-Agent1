"""OpenTelemetry observability for the agent.

Usage:
    from agent.observability import init_observability, get_tracer, get_meter

    await init_observability(config)
    tracer = get_tracer()
    with tracer.start_as_current_span("my-span") as span:
        ...
"""

from agent.observability.config import ObservabilityConfig
from agent.observability.instrumentation import record_error, record_session_start
from agent.observability.provider import (
    get_meter,
    get_tracer,
    init_observability,
    is_observability_enabled,
    shutdown_observability,
)

__all__ = [
    "ObservabilityConfig",
    "get_tracer",
    "get_meter",
    "init_observability",
    "is_observability_enabled",
    "record_error",
    "record_session_start",
    "shutdown_observability",
]
