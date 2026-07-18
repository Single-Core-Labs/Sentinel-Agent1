"""Sentinel AI agent exceptions."""


class AgentError(Exception):
    """Base exception for all agent errors."""


class ConfigError(AgentError):
    """Invalid or missing configuration."""


class ModelError(AgentError):
    """Model routing or LLM call failure."""


class ToolError(AgentError):
    """Tool execution failure."""


class ToolNotFoundError(ToolError):
    """Requested tool is not registered."""


class ToolExecutionError(ToolError):
    """Tool execution failed at runtime."""


class SessionError(AgentError):
    """Session management error."""


class ContextError(AgentError):
    """Context window or compression error."""


class CompactionError(ContextError):
    """Failed to compact context."""


class SubagentError(AgentError):
    """Subagent execution error."""


class ApprovalError(AgentError):
    """Approval-related error (timeout, rejection)."""


class DoomLoopError(AgentError):
    """Doom loop detection triggered."""
