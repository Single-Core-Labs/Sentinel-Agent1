"""Observability package stubs.

The full OpenTelemetry integration has been stripped from the checkout to
reduce external dependencies.  The agent only expects two public functions:
``init_observability`` and ``shutdown_observability``.  They are implemented as
no‑ops that log their invocation; this satisfies the imports and keeps the
runtime functional without pulling in any heavy tracing libraries.
"""

import logging
from typing import Any

logger = logging.getLogger(__name__)


def init_observability(config: Any) -> None:
    """Initialize observability.

    The real implementation would configure OpenTelemetry exporters based on
    ``config``.  In this stub we simply log the call so developers can see that
    the hook was executed.
    """
    logger.debug("init_observability called with %s", config)


def shutdown_observability() -> None:
    """Shutdown any telemetry resources.

    The stub does nothing beyond a debug log.
    """
    logger.debug("shutdown_observability called")
