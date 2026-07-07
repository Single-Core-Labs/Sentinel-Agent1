"""Subagent tools — dynamically created from the subagent registry."""

from __future__ import annotations

import logging
from typing import Any

from agent.subagents.registry import SubagentRegistry
from agent.subagents.runner import run_subagent

logger = logging.getLogger(__name__)

# Global registry shared across the application.
_registry: SubagentRegistry | None = None


def get_registry() -> SubagentRegistry:
    """Return the global subagent registry (lazily initialised)."""
    global _registry
    if _registry is None:
        _registry = SubagentRegistry()
        _registry.register_builtins()
        _registry.load_from_config_dir()
        logger.info("Subagent registry ready: %d subagents", len(_registry))
    return _registry


def create_subagent_tool_specs() -> list[dict[str, Any]]:
    """Create tool specs for all registered subagents."""
    registry = get_registry()
    return registry.list_tool_specs()


async def subagent_dispatch_handler(
    arguments: dict[str, Any],
    session: Any = None,
    tool_call_id: str | None = None,
    **_kw,
) -> tuple[str, bool]:
    """Dispatch a subagent call to the appropriate runner.

    The tool name is embedded in ``arguments`` under ``_subagent_name``,
    set by the caller (ToolRouter will have matched the tool spec name).
    """
    subagent_name = arguments.pop("_subagent_name", None)
    if not subagent_name:
        return "No subagent name specified.", False

    task = arguments.get("task", "")
    context = arguments.get("context", "")
    if not task:
        return "No task provided.", False

    if not session:
        return "No session available.", False

    registry = get_registry()
    definition = registry.get(subagent_name)
    if not definition:
        return f"Unknown subagent: {subagent_name}", False

    return await run_subagent(
        definition,
        task,
        context,
        session=session,
        tool_call_id=tool_call_id,
    )
