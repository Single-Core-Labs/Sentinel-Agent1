"""Pydantic models for API requests and responses."""

from enum import Enum
from typing import Any, Literal

from pydantic import BaseModel, Field


class OpType(str, Enum):
    """Operation types matching agent/core/agent_loop.py."""

    USER_INPUT = "user_input"
    EXEC_APPROVAL = "exec_approval"
    UNDO = "undo"
    COMPACT = "compact"
    SHUTDOWN = "shutdown"


class Operation(BaseModel):
    """Operation to be submitted to the agent."""

    op_type: OpType
    data: dict[str, Any] | None = None


class Submission(BaseModel):
    """Submission wrapper with ID and operation."""

    id: str
    operation: Operation


class ToolApproval(BaseModel):
    """Approval decision for a single tool call."""

    tool_call_id: str
    approved: bool
    feedback: str | None = None
    edited_script: str | None = None
    namespace: str | None = None


class ApprovalRequest(BaseModel):
    """Request to approve/reject tool calls."""

    session_id: str
    approvals: list[ToolApproval]


class SubmitRequest(BaseModel):
    """Request to submit user input."""

    session_id: str
    # Cap text size to prevent context-bloat / cost-amplification: a malicious
    # or runaway client could otherwise attach megabytes that then ride along
    # in every subsequent turn until /api/compact is called.
    text: str = Field(..., min_length=1, max_length=100_000)


class TruncateRequest(BaseModel):
    """Request to truncate conversation history to before a specific user message."""

    user_message_index: int


class SessionResponse(BaseModel):
    """Response when creating a new session."""

    session_id: str
    ready: bool = True
    model: str | None = None


class PendingApprovalTool(BaseModel):
    """A tool waiting for user approval."""

    tool: str
    tool_call_id: str
    arguments: dict[str, Any] = {}


class SessionAutoApprovalInfo(BaseModel):
    """Per-session auto-approval budget state."""

    enabled: bool = False
    cost_cap_usd: float | None = None
    estimated_spend_usd: float = 0.0
    remaining_usd: float | None = None


class SessionInfo(BaseModel):
    """Session metadata."""

    session_id: str
    created_at: str
    usage_window_started_at: str | None = None
    is_active: bool
    is_processing: bool = False
    message_count: int
    user_id: str = "dev"
    pending_approval: list[PendingApprovalTool] | None = None
    model: str | None = None
    title: str | None = None
    notification_destinations: list[str] = Field(default_factory=list)
    auto_approval: SessionAutoApprovalInfo = Field(
        default_factory=SessionAutoApprovalInfo
    )


class SessionNotificationsRequest(BaseModel):
    """Replace the session's auto-notification destinations."""

    destinations: list[str]


class SessionYoloRequest(BaseModel):
    """Update a session's auto-approval policy."""

    enabled: bool
    cost_cap_usd: float | None = Field(default=None, ge=0)


class UsageBucket(BaseModel):
    """App-attributed usage totals for a session."""

    session_id: str | None = None
    total_usd: float = 0.0
    inference_usd: float = 0.0
    llm_calls: int = 0
    prompt_tokens: int = 0
    completion_tokens: int = 0
    cache_read_tokens: int = 0
    cache_creation_tokens: int = 0
    total_tokens: int = 0


class UsageResponse(BaseModel):
    """Current-user app-attributed usage response."""

    source: Literal["app_telemetry"]
    currency: Literal["USD"]
    generated_at: str
    timezone: str
    session: UsageBucket | None = None
    auto_approval: SessionAutoApprovalInfo | None = None
    links: dict[str, str] = Field(default_factory=dict)


class HealthResponse(BaseModel):
    """Health check response."""

    status: str = "ok"
    active_sessions: int = 0
    max_sessions: int = 0


class LLMHealthResponse(BaseModel):
    """LLM provider health check response."""

    status: str  # "ok" | "error" | "skipped"
    model: str
    error: str | None = None
    error_type: str | None = (
        None  # "auth" | "credits" | "rate_limit" | "network" | "unknown"
    )
