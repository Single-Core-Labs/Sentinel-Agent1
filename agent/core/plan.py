"""
Structured Plan — typed representation of a multi-step plan.

Compatible with the existing plan_tool's list-of-dicts wire format.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from datetime import datetime
from enum import Enum
from typing import Literal


StepStatus = Literal["pending", "in_progress", "completed", "blocked", "skipped"]


class Phase(Enum):
    PLAN = "plan"
    ACT = "act"
    OBSERVE = "observe"


@dataclass
class PlanStep:
    id: str
    description: str
    status: StepStatus = "pending"
    dependencies: list[str] = field(default_factory=list)
    result: str | None = None
    error: str | None = None


@dataclass
class Plan:
    steps: list[PlanStep]
    goal: str = ""
    created_at: str = field(default_factory=lambda: datetime.now().isoformat())
    current_phase: Phase = Phase.PLAN

    def next_steps(self) -> list[PlanStep]:
        ready: list[PlanStep] = []
        for step in self.steps:
            if step.status != "pending":
                continue
            deps_met = all(
                s.status == "completed"
                for s in self.steps
                if s.id in step.dependencies
            )
            if deps_met:
                ready.append(step)
        return ready

    def is_complete(self) -> bool:
        return all(s.status == "completed" for s in self.steps)

    def has_unfinished(self) -> bool:
        return any(s.status in {"pending", "in_progress", "blocked"} for s in self.steps)

    def update_step(self, step_id: str, **kwargs) -> None:
        for step in self.steps:
            if step.id == step_id:
                for key, value in kwargs.items():
                    setattr(step, key, value)
                return

    def to_tool_format(self) -> list[dict[str, str]]:
        return [
            {
                "id": s.id,
                "content": s.description,
                "status": s.status,
            }
            for s in self.steps
        ]

    @classmethod
    def from_tool_format(cls, todos: list[dict[str, str]], goal: str = "") -> Plan:
        steps = []
        for item in todos:
            step = PlanStep(
                id=item.get("id", ""),
                description=item.get("content", ""),
                status=item.get("status", "pending"),
                dependencies=[],
            )
            steps.append(step)
        return Plan(steps=steps, goal=goal)

    def advance_phase(self) -> Phase | None:
        if self.current_phase == Phase.PLAN:
            self.current_phase = Phase.ACT
            return self.current_phase
        if self.current_phase == Phase.ACT:
            self.current_phase = Phase.OBSERVE
            return self.current_phase
        return None
