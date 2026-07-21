"""Instrumentation stubs for OpenTelemetry.

The production code wraps LLM calls in a span via ``instrument_llm_call``.
For the trimmed repo we provide a lightweight no‑op implementation that
exposes the same API: a context manager yielding a ``Span``‑like object with a
``set_attribute`` method.  All calls become no‑ops, ensuring that telemetry
does not interfere with normal operation while keeping the import path
intact.
"""

from contextlib import contextmanager
from typing import Any, Dict


class _DummySpan:
    """A minimal stand‑in for an OpenTelemetry Span.

    The real span supports ``set_attribute``; this dummy implementation simply
    accepts the call and discards the data.
    """

    def set_attribute(self, name: str, value: Any) -> None:  # noqa: D401
        """No‑op attribute setter.

        Args:
            name: Attribute name (ignored).
            value: Attribute value (ignored).
        """
        # Intentionally empty – the span does nothing in the stub.
        return None

    def __enter__(self):
        return self

    def __exit__(self, exc_type, exc_val, exc_tb):
        # No cleanup required for the dummy.
        return False


@contextmanager
def instrument_llm_call(**kwargs: Any):
    """Context manager that pretends to start an OpenTelemetry span.

    The real implementation records LLM‑call metrics.  Here we return a dummy
    span object that satisfies the ``with`` block used throughout the codebase.
    All keyword arguments are accepted for signature compatibility but are not
    used.
    """
    span = _DummySpan()
    try:
        yield span
    finally:
        # No real teardown required.
        pass
