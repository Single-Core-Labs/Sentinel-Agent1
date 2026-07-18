from datetime import UTC, datetime
import sys
from pathlib import Path
from types import SimpleNamespace

import pytest

_BACKEND_DIR = Path(__file__).resolve().parent.parent.parent / "backend"
if str(_BACKEND_DIR) not in sys.path:
    sys.path.insert(0, str(_BACKEND_DIR))

from usage import (  # noqa: E402
    USAGE_EVENT_TYPES,
    aggregate_usage_events,
    build_usage_response,
    resolve_usage_windows,
)
from agent.core import session_persistence  # noqa: E402
from agent.core.usage_metrics import (  # noqa: E402
    summarize_usage_events,
    usage_metric_scalar_fields,
)

BILLING_SESSION_ID = "00000000-0000-4000-8000-000000000001"


def _event(event_type, data=None, created_at="2026-06-01T12:00:00+00:00"):
    return {
        "event_type": event_type,
        "data": data or {},
        "timestamp": created_at,
    }


def test_aggregate_usage_events_sums_inference_jobs_and_sandboxes():
    events = [
        _event(
            "llm_call",
            {
                "cost_usd": 0.125,
                "prompt_tokens": 100,
                "completion_tokens": 50,
                "cache_read_tokens": 25,
                "cache_creation_tokens": 5,
                "total_tokens": 180,
            },
        ),
        _event("llm_call", {"cost_usd": 0.25, "prompt_tokens": 10}),
        _event(
            "sandbox_create",
            {
                "sandbox_id": "alice/sandbox-12345678",
                "hardware": "cpu-upgrade",
            },
            created_at="2026-06-01T12:30:00+00:00",
        ),
        _event(
            "sandbox_destroy",
            {
                "sandbox_id": "alice/sandbox-12345678",
                "lifetime_s": 3600,
            },
            created_at="2026-06-01T13:30:00+00:00",
        ),
    ]

    usage = aggregate_usage_events(events, session_id="s1")

    assert usage["session_id"] == "s1"
    assert usage["llm_calls"] == 2
    assert usage["sandbox_count"] == 1
    assert usage["prompt_tokens"] == 110
    assert usage["completion_tokens"] == 50
    assert usage["cache_read_tokens"] == 25
    assert usage["cache_creation_tokens"] == 5
    assert usage["total_tokens"] == 190
    assert usage["sandbox_billable_seconds_estimate"] == 3600
    assert usage["inference_usd"] == 0.375
    assert usage["sandbox_estimated_usd"] == 0.05
    assert usage["total_usd"] == 0.425


def test_aggregate_usage_events_treats_missing_costs_as_zero():
    usage = aggregate_usage_events(
        [
            _event("llm_call", {"prompt_tokens": 7}),
        ]
    )

    assert usage["llm_calls"] == 1
    assert usage["prompt_tokens"] == 7
    assert usage["total_usd"] == 0.0


def test_aggregate_usage_events_ignores_active_sandbox_before_destroy():
    usage = aggregate_usage_events(
        [
            _event(
                "sandbox_create",
                {
                    "sandbox_id": "alice/sandbox-12345678",
                    "hardware": "a100-large",
                },
            )
        ]
    )

    assert usage["sandbox_count"] == 0
    assert usage["sandbox_estimated_usd"] == 0.0
    assert usage["sandbox_billable_seconds_estimate"] == 0
    assert usage["total_usd"] == 0.0


def test_aggregate_usage_events_counts_cpu_basic_sandbox_as_free():
    usage = aggregate_usage_events(
        [
            _event(
                "sandbox_create",
                {
                    "sandbox_id": "alice/sandbox-12345678",
                    "hardware": "cpu-basic",
                },
            ),
            _event(
                "sandbox_destroy",
                {
                    "sandbox_id": "alice/sandbox-12345678",
                    "lifetime_s": 3600,
                },
            ),
        ]
    )

    assert usage["sandbox_count"] == 1
    assert usage["sandbox_estimated_usd"] == 0.0
    assert usage["sandbox_billable_seconds_estimate"] == 0
    assert usage["total_usd"] == 0.0


def test_aggregate_usage_events_falls_back_to_sandbox_timestamps():
    usage = aggregate_usage_events(
        [
            _event(
                "sandbox_create",
                {
                    "sandbox_id": "alice/sandbox-12345678",
                    "hardware": "t4-small",
                },
                created_at="2026-06-01T12:00:00+00:00",
            ),
            _event(
                "sandbox_destroy",
                {"sandbox_id": "alice/sandbox-12345678"},
                created_at="2026-06-01T12:30:00+00:00",
            ),
        ]
    )

    assert usage["sandbox_count"] == 1
    assert usage["sandbox_billable_seconds_estimate"] == 1800
    assert usage["sandbox_estimated_usd"] == 0.3
    assert usage["total_usd"] == 0.3


def test_sandbox_lifecycle_pairing_is_shared_for_duplicate_creates():
    events = [
        _event(
            "sandbox_create",
            {"sandbox_id": "alice/sandbox-reused", "hardware": "t4-small"},
            created_at="2026-06-01T12:00:00+00:00",
        ),
        _event(
            "sandbox_create",
            {"sandbox_id": "alice/sandbox-reused", "hardware": "cpu-basic"},
            created_at="2026-06-01T12:05:00+00:00",
        ),
        _event(
            "sandbox_destroy",
            {"sandbox_id": "alice/sandbox-reused", "lifetime_s": 300},
            created_at="2026-06-01T12:10:00+00:00",
        ),
        _event(
            "sandbox_destroy",
            {"sandbox_id": "alice/sandbox-reused", "lifetime_s": 1200},
            created_at="2026-06-01T12:20:00+00:00",
        ),
    ]

    usage = aggregate_usage_events(events, session_id="s1")
    metrics = summarize_usage_events(events, session_id="s1")

    assert usage["sandbox_count"] == 2
    assert usage["sandbox_billable_seconds_estimate"] == 1200
    assert usage["sandbox_estimated_usd"] == 0.2
    assert metrics["sandboxes"]["matched_pairs"] == usage["sandbox_count"]
    assert metrics["sandboxes"]["unpaired_creates"] == 0
    assert metrics["sandboxes"]["unpaired_destroys"] == 0
    assert metrics["sandboxes"]["estimated_usd"] == usage["sandbox_estimated_usd"]


def test_usage_event_type_allowlists_include_sandbox_lifecycle():
    assert set(USAGE_EVENT_TYPES) >= {"sandbox_create", "sandbox_destroy"}
    assert set(session_persistence.USAGE_EVENT_TYPES) >= {
        "sandbox_create",
        "sandbox_destroy",
    }


def test_summarize_usage_events_aggregates_dataset_analytics():
    events = [
        _event(
            "llm_call",
            {
                "model": "model-a",
                "kind": "main",
                "cost_usd": 0,
                "prompt_tokens": 10,
                "completion_tokens": 5,
                "cache_read_tokens": 2,
            },
        ),
        _event(
            "llm_call",
            {
                "model": "model-b",
                "kind": "research",
                "cost_usd": 0.125,
                "prompt_tokens": 20,
                "completion_tokens": 10,
                "cache_creation_tokens": 3,
                "total_tokens": 40,
            },
        ),
        _event(
            "sandbox_create",
            {"sandbox_id": "alice/sandbox-1", "hardware": "t4-small"},
            created_at="2026-06-01T12:00:00+00:00",
        ),
        _event(
            "sandbox_destroy",
            {"sandbox_id": "alice/sandbox-1", "lifetime_s": 1800},
            created_at="2026-06-01T12:30:00+00:00",
        ),
        _event(
            "sandbox_create",
            {"sandbox_id": "alice/sandbox-2", "hardware": "a100-large"},
            created_at="2026-06-01T13:00:00+00:00",
        ),
        _event(
            "sandbox_destroy",
            {"sandbox_id": "alice/sandbox-missing", "lifetime_s": 60},
            created_at="2026-06-01T13:05:00+00:00",
        ),
        _event("turn_complete"),
        _event("assistant_stream_end"),
        {"event_type": "debug", "data": {}},
    ]

    metrics = summarize_usage_events(events, session_id="s1")

    assert metrics["version"] == 1
    assert metrics["total_usd_source"] == "app_telemetry_fallback"
    assert metrics["total_usd"] == 0.425
    assert metrics["llm"] == {
        "calls": 2,
        "calls_by_kind": {"main": 1, "research": 1},
        "calls_by_model": {"model-a": 1, "model-b": 1},
        "prompt_tokens": 30,
        "completion_tokens": 15,
        "cache_read_tokens": 2,
        "cache_creation_tokens": 3,
        "total_tokens": 57,
    }
    assert metrics["turns"] == {
        "turn_complete_count": 1,
        "assistant_stream_end_count": 1,
    }
    assert metrics["sandboxes"]["creates"] == 2
    assert metrics["sandboxes"]["destroys"] == 2
    assert metrics["sandboxes"]["matched_pairs"] == 1
    assert metrics["sandboxes"]["unpaired_creates"] == 1
    assert metrics["sandboxes"]["unpaired_destroys"] == 1
    assert metrics["sandboxes"]["hardware"] == {"a100-large": 1, "t4-small": 1}
    assert metrics["sandboxes"]["estimated_usd"] == 0.3
    assert metrics["sandboxes"]["billable_seconds_estimate"] == 1800
    assert metrics["data_quality"] == {
        "event_count": 7,
        "events_without_timestamp": 1,
        "llm_calls_with_cost_usd": 2,
        "llm_calls_with_nonzero_cost_usd": 1,
    }

    assert usage_metric_scalar_fields(metrics) == {
        "usage_total_usd": 0.925,
        "usage_total_usd_source": "app_telemetry_fallback",
        "usage_app_total_usd": 0.925,
        "usage_llm_calls": 2,
        "usage_total_tokens": 57,
        "usage_sandbox_creates": 2,
        "usage_sandbox_pairs": 1,
    }


def test_usage_windows_respect_browser_timezone():
    windows = resolve_usage_windows(
        "America/Los_Angeles",
        now=datetime(2026, 6, 1, 7, 30, tzinfo=UTC),
    )

    assert windows["timezone"] == "America/Los_Angeles"
    assert windows["month_start_utc"] == datetime(2026, 6, 1, 7, 0, tzinfo=UTC)


class _NoopStore:
    enabled = False


class _RecordingStore:
    enabled = True

    def __init__(self):
        self.calls = []

    async def load_usage_events(self, user_id, **kwargs):
        self.calls.append((user_id, kwargs))
        return []


class _Manager:
    def __init__(self, sessions, store=None):
        self.sessions = sessions
        self.store = store or _NoopStore()

    def _store(self):
        return self.store


class _MetadataStore(_NoopStore):
    enabled = True

    def __init__(self, metadata):
        self.metadata = metadata

    async def load_session(self, session_id):
        return {"metadata": {"session_id": session_id, **self.metadata}, "messages": []}

    async def load_usage_events(self, user_id, **kwargs):
        return []


def _agent_session(session_id, user_id, events):
    return SimpleNamespace(
        session_id=session_id,
        user_id=user_id,
        inference_billing_session_id=BILLING_SESSION_ID,
        session=SimpleNamespace(logged_events=events),
    )


@pytest.mark.asyncio
async def test_usage_response_omits_app_rollups_without_session():
    manager = _Manager(
        {
            "owner-session": _agent_session(
                "owner-session",
                "owner",
                [_event("llm_call", {"cost_usd": 0.5})],
            ),
            "other-session": _agent_session(
                "other-session",
                "other",
                [_event("llm_call", {"cost_usd": 99.0})],
            ),
        }
    )

    usage = await build_usage_response(
        manager,
        user_id="owner",
        session_id=None,
        timezone_name="UTC",
        now=datetime(2026, 6, 1, 13, 0, tzinfo=UTC),
    )

    assert usage["session"] is None


@pytest.mark.asyncio
async def test_runtime_usage_includes_requested_session_total():
    manager = _Manager(
        {
            "s1": _agent_session(
                "s1",
                "owner",
                [
                    _event(
                        "llm_call",
                        {"cost_usd": 0.25},
                        created_at="2026-05-01T12:00:00+00:00",
                    )
                ],
            )
        }
    )

    usage = await build_usage_response(
        manager,
        user_id="owner",
        session_id="s1",
        timezone_name="UTC",
        now=datetime(2026, 6, 1, 13, 0, tzinfo=UTC),
    )

    assert usage["session"]["session_id"] == "s1"
    assert usage["session"]["inference_usd"] == 0.25


@pytest.mark.asyncio
async def test_runtime_usage_includes_requested_session_tokens():
    manager = _Manager(
        {
            "s1": _agent_session(
                "s1",
                "owner",
                [
                    _event(
                        "llm_call",
                        {"cost_usd": 0.25, "total_tokens": 42},
                        created_at="2026-06-05T15:00:00",
                    )
                ],
            )
        }
    )

    usage = await build_usage_response(
        manager,
        user_id="owner",
        session_id="s1",
        timezone_name="Europe/Zurich",
        now=datetime(2026, 6, 5, 13, 30, tzinfo=UTC),
    )

    assert usage["session"]["llm_calls"] == 1
    assert usage["session"]["total_tokens"] == 42


@pytest.mark.asyncio
async def test_usage_response_loads_only_session_events(monkeypatch):
    session_created_at = datetime(2026, 6, 5, 12, 0, tzinfo=UTC)
    store = _RecordingStore()
    manager = _Manager(
        {
            "s1": SimpleNamespace(
                session_id="s1",
                user_id="owner",
                created_at=session_created_at,
                inference_billing_session_id=BILLING_SESSION_ID,
                session=SimpleNamespace(logged_events=[]),
            )
        },
        store=store,
    )
    _ = await build_usage_response(
        manager,
        user_id="owner",
        session_id="s1",
        timezone_name="UTC",
        now=datetime(2026, 6, 5, 13, 0, tzinfo=UTC),
    )

    assert store.calls == [
        (
            "owner",
            {"session_id": "s1", "start": session_created_at, "end": None},
        )
    ]
