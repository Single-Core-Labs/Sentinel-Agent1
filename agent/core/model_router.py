"""
Model Router — switches between cheap (planning) and strong (execution) models.

Each mode has an optional fallback: when set, the probe cascade targets the
fallback model if the primary rejects the effort level.
"""

from __future__ import annotations

import logging
from dataclasses import dataclass

logger = logging.getLogger(__name__)


@dataclass
class ModelRoute:
    primary: str
    label: str = ""
    fallback: str | None = None


class ModelRouter:
    """Routes between cheap and strong models."""

    def __init__(
        self,
        strong_model: str,
        cheap_model: str | None = None,
        strong_fallback: str | None = None,
        cheap_fallback: str | None = None,
    ):
        self._routes: dict[str, ModelRoute] = {
            "strong": ModelRoute(
                primary=strong_model,
                label="strong",
                fallback=strong_fallback,
            ),
            "cheap": ModelRoute(
                primary=cheap_model or strong_model,
                label="cheap",
                fallback=cheap_fallback,
            ),
        }
        self._current: str = "strong"

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

    def summary(self) -> dict:
        return {
            "current": self._current,
            "current_model": self.current_model,
            "strong_model": self.strong_model,
            "cheap_model": self.cheap_model,
        }
