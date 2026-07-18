"""Shared type definitions for the Sentinel AI agent."""

from typing import Any
from dataclasses import dataclass
from enum import Enum


class OpType(Enum):
    """Operation types for the agent submission queue."""
    USER_INPUT = "user_input"
    EXEC_APPROVAL = "exec_approval"
    UNDO = "undo"
    REDO = "redo"
    COMPACT = "compact"
    NEW = "new"
    RESUME = "resume"
    SHUTDOWN = "shutdown"


@dataclass
class Event:
    """Agent event for the output queue."""
    type: str
    data: dict[str, Any] | None = None
    timestamp: float | None = None


from .errors import (
    AgentError,
    ApprovalError,
    CompactionError,
    ConfigError,
    ContextError,
    DoomLoopError,
    ModelError,
    SessionError,
    SubagentError,
    ToolError,
    ToolExecutionError,
    ToolNotFoundError,
)

__all__ = [
    "OpType",
    "Event",
    "AgentError",
    "ConfigError",
    "ModelError",
    "ToolError",
    "ToolNotFoundError",
    "ToolExecutionError",
    "SessionError",
    "ContextError",
    "CompactionError",
    "SubagentError",
    "ApprovalError",
    "DoomLoopError",
]
