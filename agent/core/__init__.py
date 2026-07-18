"""
Core agent implementation
Contains the main agent logic, decision-making, and orchestration
"""

from agent.core.agent_loop import Handlers, process_submission, submission_loop
from agent.core.model_ids import strip_sentinel_ai_model_prefix
from agent.core.model_router import (
    ClassificationRule,
    ModelRouter,
    RoutingAuditEntry,
    StepClassifier,
)
from agent.core.plan import Phase, Plan, PlanStep
from agent.core.session import Session
from agent.core.tools import ToolRouter, ToolSpec, create_builtin_tools

__all__ = [
    "ClassificationRule",
    "Handlers",
    "ModelRouter",
    "Phase",
    "Plan",
    "PlanStep",
    "RoutingAuditEntry",
    "Session",
    "StepClassifier",
    "ToolRouter",
    "ToolSpec",
    "create_builtin_tools",
    "process_submission",
    "strip_sentinel_ai_model_prefix",
    "submission_loop",
]
