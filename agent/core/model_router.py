"""
Model Router — classifies planned steps as mechanical or reasoning and routes
to the appropriate model. Keeps an audit log of every routing decision.
"""

from __future__ import annotations

import logging
import re
from dataclasses import dataclass, field
from datetime import datetime


logger = logging.getLogger(__name__)


# ── Classification -----------------------------------------------------------

@dataclass
class ClassificationRule:
    pattern: re.Pattern
    category: str  # "mechanical" | "reasoning"
    priority: int = 0


MECHANICAL_PATTERNS: list[str] = [
    r"\blist\b", r"\bls\b", r"\bgrep\b", r"\bsearch\b", r"\bfind\b",
    r"\bformat(?:ting)?\b", r"\blint\b", r"\bcheck\b", r"\bcount\b",
    r"\bsort\b", r"\bread\b", r"\bstat\b", r"\bglob\b", r"\blookup\b",
    r"\bcat\b", r"\bhead\b", r"\btail\b", r"\bwc\b", r"\bdu\b",
]

REASONING_PATTERNS: list[str] = [
    r"\bplan\b", r"\bdesign\b", r"\bdecide?\b", r"\bdebug\b",
    r"\bdiagnos(?:e|is)\b", r"\barchitect(?:ure)?\b", r"\brefactor\b",
    r"\boptimize?\b", r"\banaly(?:ze|sis)\b", r"\binvestigat\w+\b",
    r"\broot cause\b", r"\bstrategy\b", r"\bchoose?\b", r"\bcompare?\b",
    r"\bevaluat\w+\b", r"\barchitect\b", r"\btrade.?off\b",
    r"\bdecide?\b", r"\bresolv\w+\b",
]


def _default_rules() -> list[ClassificationRule]:
    rules: list[ClassificationRule] = []
    for i, pat in enumerate(MECHANICAL_PATTERNS):
        rules.append(ClassificationRule(pattern=re.compile(pat, re.I), category="mechanical", priority=i))
    for i, pat in enumerate(REASONING_PATTERNS):
        rules.append(ClassificationRule(pattern=re.compile(pat, re.I), category="reasoning", priority=i + 100))
    return rules


class StepClassifier:
    """Classifies step descriptions as mechanical or reasoning.

    Rules are matched in priority order. The first matching rule wins.
    An unrecognised description defaults to "reasoning" (the safer choice).
    """

    def __init__(self, rules: list[ClassificationRule] | None = None):
        self._rules: list[ClassificationRule] = sorted(
            rules or _default_rules(),
            key=lambda r: r.priority,
        )

    @classmethod
    def from_keywords(
        cls,
        mechanical: list[str] | None = None,
        reasoning: list[str] | None = None,
    ) -> StepClassifier:
        """Build a classifier from plain keyword lists (each keyword is wrapped
        in ``\\b<word>\\b``)."""
        rules: list[ClassificationRule] = []
        if mechanical:
            for i, kw in enumerate(mechanical):
                rules.append(ClassificationRule(pattern=re.compile(rf"\b{re.escape(kw)}\b", re.I), category="mechanical", priority=i))
        if reasoning:
            for i, kw in enumerate(reasoning):
                rules.append(ClassificationRule(pattern=re.compile(rf"\b{re.escape(kw)}\b", re.I), category="reasoning", priority=i + 100))
        return cls(rules=rules if rules else None)

    def classify(self, description: str) -> str:
        """Return ``"mechanical"`` or ``"reasoning"``."""
        if not description:
            return "reasoning"
        for rule in self._rules:
            if rule.pattern.search(description):
                logger.debug("classifier: '%s' -> %s (rule: %s)", description[:60], rule.category, rule.pattern.pattern)
                return rule.category
        logger.debug("classifier: '%s' -> default reasoning (no rule matched)", description[:60])
        return "reasoning"

    @property
    def rules(self) -> list[ClassificationRule]:
        return list(self._rules)


# ── Audit log ----------------------------------------------------------------

@dataclass
class RoutingAuditEntry:
    step_id: str
    step_description: str
    classification: str
    assigned_model: str
    timestamp: str = field(default_factory=lambda: datetime.now().isoformat())


# ── Enhanced Model Router ----------------------------------------------------

@dataclass
class ModelRoute:
    primary: str
    label: str = ""
    fallback: str | None = None


_ROUTING_MODE_MECHANICAL = "mechanical"
_ROUTING_MODE_REASONING = "reasoning"


class ModelRouter:
    """Step-aware model router.

    Classifies each planned step and routes it to the appropriate model.
    Every routing decision is logged in ``audit_log`` for cost auditing.
    """

    def __init__(
        self,
        strong_model: str,
        cheap_model: str | None = None,
        strong_fallback: str | None = None,
        cheap_fallback: str | None = None,
        classifier: StepClassifier | None = None,
    ):
        self._routes: dict[str, ModelRoute] = {
            "strong": ModelRoute(primary=strong_model, label="strong", fallback=strong_fallback),
            "cheap": ModelRoute(primary=cheap_model or strong_model, label="cheap", fallback=cheap_fallback),
        }
        self._current: str = "strong"
        self.classifier: StepClassifier = classifier or StepClassifier()
        self.audit_log: list[RoutingAuditEntry] = []

        # Default mode-to-route mapping (overridable via configure()).
        self._mode_map: dict[str, str] = {
            _ROUTING_MODE_MECHANICAL: "cheap",
            _ROUTING_MODE_REASONING: "strong",
        }

    # ── Properties ──────────────────────────────────────────────────────────

    @property
    def current_route(self) -> str:
        return self._current

    @property
    def current_model(self) -> str:
        return self._routes[self._current].primary

    @property
    def strong_model(self) -> str:
        return self._routes["strong"].primary

    @property
    def cheap_model(self) -> str:
        return self._routes["cheap"].primary

    # ── Manual switching ────────────────────────────────────────────────────

    def use_strong(self) -> None:
        self._current = "strong"
        logger.info("Model router switched to strong: %s", self.strong_model)

    def use_cheap(self) -> None:
        self._current = "cheap"
        logger.info("Model router switched to cheap: %s", self.cheap_model)

    def use_model(self, model: str) -> None:
        for route_key, route in self._routes.items():
            if route.primary == model:
                self._current = route_key
                return
        self._routes["strong"] = ModelRoute(primary=model, label="strong")
        self._current = "strong"

    def is_cheap(self) -> bool:
        return self._current == "cheap"

    def is_strong(self) -> bool:
        return self._current == "strong"

    # ── Configuration ──────────────────────────────────────────────────────

    def configure(
        self,
        *,
        mechanical_route: str | None = None,
        reasoning_route: str | None = None,
        overrides: dict[str, str] | None = None,
    ) -> None:
        """Override the default mode-to-route mapping.

        ``overrides`` takes precedence: key = step id, value = route name.
        """
        if mechanical_route is not None:
            self._mode_map[_ROUTING_MODE_MECHANICAL] = mechanical_route
        if reasoning_route is not None:
            self._mode_map[_ROUTING_MODE_REASONING] = reasoning_route
        self._step_overrides: dict[str, str] = overrides or getattr(self, "_step_overrides", {})

    # ── The main routing method ────────────────────────────────────────────

    def route_for_step(self, step_id: str, description: str) -> str:
        """Classify a step and return the model id that should handle it.

        Also switches ``_current`` so subsequent non-step calls use the
        same model.  Every decision is appended to ``audit_log``.
        """
        # 1. Per-step override (highest priority)
        overrides: dict[str, str] = getattr(self, "_step_overrides", {})
        if step_id in overrides:
            route_key = overrides[step_id]
            route = self._routes.get(route_key)
            if route is not None:
                self._current = route_key
                self._log_decision(step_id, description, f"override:{route_key}", route.primary)
                return route.primary

        # 2. Classify the step
        category = self.classifier.classify(description)
        route_key = self._mode_map.get(category, "strong")
        route = self._routes.get(route_key)
        model_id = route.primary if route else self.current_model
        self._current = route_key
        self._log_decision(step_id, description, category, model_id)
        return model_id

    def _log_decision(self, step_id: str, description: str, classification: str, model: str) -> None:
        entry = RoutingAuditEntry(
            step_id=step_id,
            step_description=description,
            classification=classification,
            assigned_model=model,
        )
        self.audit_log.append(entry)
        logger.info(
            "ROUTE step=%s class=%s model=%s desc=%s",
            step_id, classification, model, description[:80],
        )

    # ── Utilities ──────────────────────────────────────────────────────────

    def summary(self) -> dict:
        return {
            "current": self._current,
            "current_model": self.current_model,
            "strong_model": self.strong_model,
            "cheap_model": self.cheap_model,
            "audit_count": len(self.audit_log),
        }

    def audit_summary(self) -> list[dict]:
        """Return the audit log as a list of dicts (serialisable)."""
        return [
            {
                "step_id": e.step_id,
                "step_description": e.step_description,
                "classification": e.classification,
                "assigned_model": e.assigned_model,
                "timestamp": e.timestamp,
            }
            for e in self.audit_log
        ]
