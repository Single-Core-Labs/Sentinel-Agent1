"""No-op telemetry stub."""

import time
from typing import Any


async def record_llm_call(
    session: Any,
    model: str = "",
    response: Any = None,
    latency_ms: int = 0,
    finish_reason: str | None = None,
    kind: str = "",
) -> dict[str, Any]:
    return {}


async def record_pro_conversion(*args: Any, **kwargs: Any) -> None:
    pass


async def record_pro_cta_click(*args: Any, **kwargs: Any) -> None:
    pass


async def record_feedback(*args: Any, **kwargs: Any) -> None:
    pass


async def record_sandbox_create(*args: Any, **kwargs: Any) -> None:
    pass


async def record_sandbox_destroy(*args: Any, **kwargs: Any) -> None:
    pass


class HeartbeatSaver:
    maybe_fire = staticmethod(lambda _: None)
    time = time
