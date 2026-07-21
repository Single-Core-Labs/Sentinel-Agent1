"""Pure usage/billing summaries for session trajectory analytics."""

from collections import Counter
from datetime import UTC, datetime
from math import isfinite
from typing import Any

USAGE_METRICS_VERSION = 1

_USAGE_SCALAR_KEYS = (
    "usage_total_usd",
    "usage_total_usd_source",
    "usage_app_total_usd",
    "usage_llm_calls",
    "usage_total_tokens",
)


def _coerce_float(value: Any) -> float:
    if isinstance(value, bool) or value is None:
        return 0.0
    try:
        parsed = float(value)
    except (TypeError, ValueError):
        return 0.0
    return parsed if isfinite(parsed) else 0.0


def _coerce_optional_float(value: Any) -> float | None:
    if isinstance(value, bool) or value is None:
        return None
    try:
        parsed = float(value)
    except (TypeError, ValueError):
        return None
    return parsed if isfinite(parsed) else None


def _coerce_int(value: Any) -> int:
    if isinstance(value, bool) or value is None:
        return 0
    try:
        return int(value)
    except (TypeError, ValueError):
        return 0


def _round_usd(value: Any) -> float:
    return round(_coerce_float(value), 6)


def _parse_timestamp(value: Any) -> datetime | None:
    if isinstance(value, datetime):
        dt = value
    elif isinstance(value, str) and value:
        try:
            dt = datetime.fromisoformat(value.replace("Z", "+00:00"))
        except ValueError:
            return None
    else:
        return None
    if dt.tzinfo is None:
        return dt.replace(tzinfo=UTC)
    return dt.astimezone(UTC)


def event_created_at(event: dict[str, Any]) -> datetime | None:
    return _parse_timestamp(event.get("created_at") or event.get("timestamp"))


def _event_data(event: dict[str, Any]) -> dict[str, Any]:
    data = event.get("data") or {}
    return data if isinstance(data, dict) else {}


def _has_number(value: Any) -> bool:
    return _coerce_optional_float(value) is not None


def _counter_dict(counter: Counter[str]) -> dict[str, int]:
    return dict(sorted(counter.items()))


def _empty_app_bucket(session_id: str | None) -> dict[str, Any]:
    return {
        "session_id": session_id,
        "total_usd": 0.0,
        "inference_usd": 0.0,
        "llm_calls": 0,
        "prompt_tokens": 0,
        "completion_tokens": 0,
        "cache_read_tokens": 0,
        "cache_creation_tokens": 0,
        "total_tokens": 0,
    }



def summarize_usage_events(
    events: list[dict[str, Any]],
    *,
    session_id: str | None = None,
) -> dict[str, Any]:
    app = _empty_app_bucket(session_id)
    llm_by_kind: Counter[str] = Counter()
    llm_by_model: Counter[str] = Counter()

    event_count = 0
    events_without_timestamp = 0
    llm_calls_with_cost_usd = 0
    llm_calls_with_nonzero_cost_usd = 0
    turn_complete_count = 0
    assistant_stream_end_count = 0

    for event in events or []:
        if not isinstance(event, dict):
            continue
        event_count += 1
        if event_created_at(event) is None:
            events_without_timestamp += 1

        event_type = event.get("event_type")
        data = _event_data(event)
        if event_type == "llm_call":
            app["llm_calls"] += 1
            if "cost_usd" in data:
                llm_calls_with_cost_usd += 1
            cost_usd = _coerce_float(data.get("cost_usd"))
            if cost_usd > 0:
                llm_calls_with_nonzero_cost_usd += 1
            app["inference_usd"] += cost_usd

            prompt_tokens = _coerce_int(data.get("prompt_tokens"))
            completion_tokens = _coerce_int(data.get("completion_tokens"))
            cache_read_tokens = _coerce_int(data.get("cache_read_tokens"))
            cache_creation_tokens = _coerce_int(data.get("cache_creation_tokens"))
            total_tokens = _coerce_int(data.get("total_tokens")) or (
                prompt_tokens
                + completion_tokens
                + cache_read_tokens
                + cache_creation_tokens
            )
            app["prompt_tokens"] += prompt_tokens
            app["completion_tokens"] += completion_tokens
            app["cache_read_tokens"] += cache_read_tokens
            app["cache_creation_tokens"] += cache_creation_tokens
            app["total_tokens"] += total_tokens
            llm_by_kind[str(data.get("kind") or "unknown")] += 1
            llm_by_model[str(data.get("model") or "unknown")] += 1
        elif event_type == "turn_complete":
            turn_complete_count += 1
        elif event_type == "assistant_stream_end":
            assistant_stream_end_count += 1

    app["inference_usd"] = _round_usd(app["inference_usd"])
    app["total_usd"] = _round_usd(app["inference_usd"])

    usage_total = app["total_usd"]
    usage_total_source = "app_telemetry_fallback"

    return {
        "version": USAGE_METRICS_VERSION,
        "session_id": session_id,
        "total_usd": usage_total,
        "total_usd_source": usage_total_source,
        "app_total_usd": app["total_usd"],
        "app_telemetry": app,
        "llm": {
            "calls": app["llm_calls"],
            "calls_by_kind": _counter_dict(llm_by_kind),
            "calls_by_model": _counter_dict(llm_by_model),
            "prompt_tokens": app["prompt_tokens"],
            "completion_tokens": app["completion_tokens"],
            "cache_read_tokens": app["cache_read_tokens"],
            "cache_creation_tokens": app["cache_creation_tokens"],
            "total_tokens": app["total_tokens"],
        },
        "turns": {
            "turn_complete_count": turn_complete_count,
            "assistant_stream_end_count": assistant_stream_end_count,
        },
        "data_quality": {
            "event_count": event_count,
            "events_without_timestamp": events_without_timestamp,
            "llm_calls_with_cost_usd": llm_calls_with_cost_usd,
            "llm_calls_with_nonzero_cost_usd": llm_calls_with_nonzero_cost_usd,
        },
    }


def usage_metric_scalar_fields(metrics: dict[str, Any]) -> dict[str, Any]:
    app = metrics.get("app_telemetry") if isinstance(metrics, dict) else {}
    llm = metrics.get("llm") if isinstance(metrics, dict) else {}
    values = {
        "usage_total_usd": metrics.get("total_usd"),
        "usage_total_usd_source": metrics.get("total_usd_source"),
        "usage_app_total_usd": metrics.get("app_total_usd"),
        "usage_llm_calls": app.get("llm_calls") if isinstance(app, dict) else None,
        "usage_total_tokens": llm.get("total_tokens")
        if isinstance(llm, dict)
        else None,
    }
    return {key: values.get(key) for key in _USAGE_SCALAR_KEYS}
