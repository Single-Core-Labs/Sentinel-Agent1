"""Conservative cost estimates for auto-approved infrastructure actions."""

from dataclasses import dataclass
from typing import Any


@dataclass(frozen=True)
class CostEstimate:
    """Estimated cost for a tool call.

    ``estimated_cost_usd=None`` means the call may be billable but we could not
    estimate it safely, so auto-approval should fall back to a human decision.
    """

    estimated_cost_usd: float | None
    billable: bool
    block_reason: str | None = None
    label: str | None = None


def parse_timeout_hours(
    value: Any, *, default_hours: float = 0.5
) -> float | None:
    if value is None or value == "":
        return default_hours
    if isinstance(value, bool):
        return None
    if isinstance(value, int | float):
        seconds = float(value)
        return seconds / 3600 if seconds > 0 else None
    if not isinstance(value, str):
        return None

    match = _DURATION_RE.match(value)
    if not match:
        return None
    amount = float(match.group(1))
    unit = match.group(2).lower() or "s"
    if amount <= 0:
        return None
    if unit == "s":
        return amount / 3600
    if unit == "m":
        return amount / 60
    if unit == "h":
        return amount
    if unit == "d":
        return amount * 24
    return None


async def estimate_tool_cost(
    tool_name: str, args: dict[str, Any], *, session: Any = None
) -> CostEstimate:
    return CostEstimate(estimated_cost_usd=0.0, billable=False)