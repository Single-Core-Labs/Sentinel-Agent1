"""Session manager for handling multiple concurrent agent sessions."""

import asyncio
import logging
import os
import uuid
from dataclasses import dataclass, field
from datetime import datetime, timedelta
from pathlib import Path
from typing import Any, Optional

from agent.core.session import Event, OpType, Session
from agent.core.tools import ToolRouter
from agent.core.usage_thresholds import (
    USAGE_WARNING_FIRST_THRESHOLD_USD,
)

# Get project root (parent of backend directory)
PROJECT_ROOT = Path(__file__).parent.parent
DEFAULT_CONFIG_PATH = str(PROJECT_ROOT / "configs" / "frontend_agent_config.json")
USAGE_WARNING_SPEND_CACHE_TTL_SECONDS = 30.0
USAGE_BILLING_REFRESH_TIMEOUT_SECONDS = 2.0


# These dataclasses match agent/main.py structure
@dataclass
class Operation:
    """Operation to be executed by the agent."""

    op_type: OpType
    data: Optional[dict[str, Any]] = None


@dataclass
class Submission:
    """Submission to the agent loop."""

    id: str
    operation: Operation


logger = logging.getLogger(__name__)


class EventBroadcaster:
    """Reads from the agent's event queue and fans out to SSE subscribers.

    Events that arrive when no subscribers are listening are discarded by
    this in-memory fanout. Durable replay is handled by session_persistence.
    """

    def __init__(self, event_queue: asyncio.Queue):
        self._source = event_queue
        self._subscribers: dict[int, asyncio.Queue] = {}
        self._counter = 0

    def subscribe(self) -> tuple[int, asyncio.Queue]:
        """Create a new subscriber. Returns (id, queue)."""
        self._counter += 1
        sub_id = self._counter
        q: asyncio.Queue = asyncio.Queue()
        self._subscribers[sub_id] = q
        return sub_id, q

    def unsubscribe(self, sub_id: int) -> None:
        self._subscribers.pop(sub_id, None)

    async def run(self) -> None:
        """Main loop — reads from source queue and broadcasts."""
        while True:
            try:
                event: Event = await self._source.get()
                msg = {
                    "event_type": event.event_type,
                    "data": event.data,
                    "seq": event.seq,
                }
                for q in self._subscribers.values():
                    await q.put(msg)
            except asyncio.CancelledError:
                break
            except Exception as e:
                logger.error(f"EventBroadcaster error: {e}")


@dataclass
class AgentSession:
    """Wrapper for an agent session with its associated resources."""

    session_id: str
    session: Session
    tool_router: ToolRouter
    submission_queue: asyncio.Queue
    user_id: str = "dev"  # Owner of this session
    task: asyncio.Task | None = None
    created_at: datetime = field(default_factory=datetime.utcnow)
    # Last genuine activity (submit/turn-start/turn-finish/direct user write).
    # Drives the idle reaper. Defaults to load time so a freshly-restored but
    # untouched session isn't reaped for a full idle window.
    last_active_at: datetime = field(default_factory=datetime.utcnow)
    is_active: bool = True
    is_processing: bool = False  # True while a submission is being executed
    # Set under the lock by the reaper while tearing this session down. Blocks
    # submit() from enqueueing onto a session that's being evicted.
    is_reaping: bool = False
    broadcaster: Any = None
    title: str | None = None
    usage_window_started_at: datetime | None = None
    inference_billing_session_id: str | None = None
    usage_warning_next_threshold_usd: float = USAGE_WARNING_FIRST_THRESHOLD_USD
    usage_warning_spend_cache: dict[str, Any] = field(default_factory=dict)

    def __post_init__(self) -> None:
        if self.usage_window_started_at is None:
            self.usage_window_started_at = self.created_at
        if not self.inference_billing_session_id or not _is_uuid(
            self.inference_billing_session_id
        ):
            self.inference_billing_session_id = new_inference_billing_session_id(
                self.session_id,
                self.usage_window_started_at,
            )
        try:
            self.session.inference_billing_session_id = (
                self.inference_billing_session_id
            )
        except AttributeError:
            pass


def new_inference_billing_session_id(
    session_id: str,  # noqa: ARG001 - kept for a stable call signature.
    started_at: datetime | None = None,  # noqa: ARG001 - kept for a stable call signature.
) -> str:
    """Return a Router billing session ID scoped to one visible usage window."""
    return str(uuid.uuid4())


def _is_uuid(value: str) -> bool:
    try:
        uuid.UUID(value)
    except ValueError:
        return False
    return True


class SessionCapacityError(Exception):
    """Raised when no more sessions can be created."""

    def __init__(self, message: str, error_type: str = "global") -> None:
        super().__init__(message)
        self.error_type = error_type  # "global" or "per_user"


# ── Capacity limits ─────────────────────────────────────────────────
# Each session uses ~10-20 MB (context, tools, queues, task); 200 × 20 MB
# = 4 GB worst case.
MAX_SESSIONS: int = 200
MAX_SESSIONS_PER_USER: int = 10
DEFAULT_YOLO_COST_CAP_USD: float = 5.0

# ── Idle-session reaper ─────────────────────────────────────────────
# A live session idle ≥ REAPER_IDLE_MINUTES with no in-flight work is
# evicted from the live pool, while staying fully resumable from Mongo.
# This frees both the global pool and the user's concurrent slots.
REAPER_IDLE_MINUTES: float = float(os.environ.get("REAPER_IDLE_MINUTES", "15"))
REAPER_INTERVAL_S: float = float(os.environ.get("REAPER_INTERVAL_S", "300"))
REAP_TEARDOWN_TIMEOUT_S: float = float(os.environ.get("REAP_TEARDOWN_TIMEOUT_S", "30"))
REAPER_IDLE = timedelta(minutes=REAPER_IDLE_MINUTES)
