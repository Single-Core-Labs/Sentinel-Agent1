"""Subagent framework — config-driven delegated agents with isolated context."""

from agent.subagents.registry import SubagentRegistry, SubagentDefinition
from agent.subagents.runner import run_subagent

__all__ = ["SubagentRegistry", "SubagentDefinition", "run_subagent"]
