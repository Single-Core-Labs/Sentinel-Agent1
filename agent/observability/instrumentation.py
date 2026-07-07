"""OTel instrumentation helpers — wrap agent operations with spans and metrics.

Every function in this module is a no-op when observability is disabled,
so callsites can call them unconditionally.
"""

from __future__ import annotations

import time
from contextlib import contextmanager
from typing import Any, Generator

from opentelemetry import trace as otel_trace
from opentelemetry.metrics import Counter, Histogram

from agent.observability.provider import get_meter, get_tracer

# ---------------------------------------------------------------------------
# Lazy-initialised metric instruments (created once on first use)
# ---------------------------------------------------------------------------

_llm_call_counter: Counter | None = None
_llm_token_counter: Counter | None = None
_llm_duration_histogram: Histogram | None = None
_tool_call_counter: Counter | None = None
_tool_duration_histogram: Histogram | None = None
_session_counter: Counter | None = None
_error_counter: Counter | None = None


def _ensure_instruments() -> None:
    global _llm_call_counter, _llm_token_counter, _llm_duration_histogram
    global _tool_call_counter, _tool_duration_histogram
    global _session_counter, _error_counter

    meter = get_meter()
    if meter is None:
        return

    if _llm_call_counter is not None:
        return

    _llm_call_counter = meter.create_counter(
        name="agent.llm.call.count",
        description="Count of LLM calls",
        unit="1",
    )
    _llm_token_counter = meter.create_counter(
        name="agent.llm.token.usage",
        description="Tokens used by LLM calls",
        unit="1",
    )
    _llm_duration_histogram = meter.create_histogram(
        name="agent.llm.call.duration",
        description="LLM call duration in milliseconds",
        unit="ms",
    )
    _tool_call_counter = meter.create_counter(
        name="agent.tool.call.count",
        description="Count of tool calls",
        unit="1",
    )
    _tool_duration_histogram = meter.create_histogram(
        name="agent.tool.call.duration",
        description="Tool call duration in milliseconds",
        unit="ms",
    )
    _session_counter = meter.create_counter(
        name="agent.session.count",
        description="Count of sessions",
        unit="1",
    )
    _error_counter = meter.create_counter(
        name="agent.error.count",
        description="Count of errors",
        unit="1",
    )


# ---------------------------------------------------------------------------
# LLM call instrumentation
# ---------------------------------------------------------------------------


@contextmanager
def instrument_llm_call(
    *,
    model: str,
    kind: str = "main",
    prompt_tokens: int | None = None,
    completion_tokens: int | None = None,
    total_tokens: int | None = None,
    finish_reason: str | None = None,
    cost_usd: float | None = None,
    latency_ms: int | None = None,
) -> Generator[otel_trace.Span | None, Any, Any]:
    """Context manager that creates an OTel span and records metrics for an LLM call.

    Usage:
        with instrument_llm_call(model="gpt-4", kind="main") as span:
            response = await acompletion(...)
            if span:
                span.set_attribute("output_tokens", ...)
    """
    _ensure_instruments()

    tracer = get_tracer()
    span = None
    if tracer is not None:
        attrs = {
            "model": model,
            "kind": kind,
        }
        if prompt_tokens is not None:
            attrs["prompt_tokens"] = prompt_tokens
        if completion_tokens is not None:
            attrs["completion_tokens"] = completion_tokens
        if finish_reason:
            attrs["finish_reason"] = finish_reason

        span = tracer.start_span(f"llm.{kind}", attributes=attrs)

    _start = time.monotonic()

    try:
        yield span
    except Exception as exc:
        if span:
            span.record_exception(exc)
            span.set_status(otel_trace.Status(otel_trace.StatusCode.ERROR, str(exc)))
        raise
    finally:
        if span:
            span.end()

        if _llm_call_counter is not None:
            _llm_call_counter.add(
                1,
                attributes={
                    "model": model,
                    "kind": kind,
                    "finish_reason": finish_reason or "unknown",
                },
            )

        if _llm_duration_histogram is not None:
            elapsed = latency_ms or int((time.monotonic() - _start) * 1000)
            _llm_duration_histogram.record(
                elapsed,
                attributes={"model": model, "kind": kind},
            )

        if _llm_token_counter is not None:
            token_attrs = {"model": model, "kind": kind}
            if prompt_tokens:
                _llm_token_counter.add(prompt_tokens, attributes={**token_attrs, "token_type": "input"})
            if completion_tokens:
                _llm_token_counter.add(completion_tokens, attributes={**token_attrs, "token_type": "output"})


# ---------------------------------------------------------------------------
# Tool call instrumentation
# ---------------------------------------------------------------------------


@contextmanager
def instrument_tool_call(
    *,
    tool_name: str,
    tool_type: str = "builtin",
) -> Generator[otel_trace.Span | None, Any, Any]:
    """Context manager that creates an OTel span for a tool call."""
    _ensure_instruments()

    tracer = get_tracer()
    span = None
    if tracer is not None:
        span = tracer.start_span(
            f"tool.{tool_name}",
            attributes={"tool.name": tool_name, "tool.type": tool_type},
        )

    _start = time.monotonic()
    try:
        yield span
    except Exception as exc:
        if span:
            span.record_exception(exc)
            span.set_status(otel_trace.Status(otel_trace.StatusCode.ERROR, str(exc)))
        raise
    finally:
        if span:
            span.end()

        if _tool_call_counter is not None:
            _tool_call_counter.add(
                1,
                attributes={"tool.name": tool_name, "tool.type": tool_type},
            )

        if _tool_duration_histogram is not None:
            elapsed = int((time.monotonic() - _start) * 1000)
            _tool_duration_histogram.record(
                elapsed,
                attributes={"tool.name": tool_name},
            )


# ---------------------------------------------------------------------------
# Session instrumentation
# ---------------------------------------------------------------------------


def record_session_start(attributes: dict[str, Any] | None = None) -> None:
    """Emit a session-start metric."""
    _ensure_instruments()
    if _session_counter is not None:
        _session_counter.add(1, attributes=attributes or {})


def record_error(
    error_type: str,
    details: str | None = None,
    attributes: dict[str, Any] | None = None,
) -> None:
    """Emit an error counter metric."""
    _ensure_instruments()
    if _error_counter is not None:
        attrs = {"error.type": error_type, **(attributes or {})}
        _error_counter.add(1, attributes=attrs)
