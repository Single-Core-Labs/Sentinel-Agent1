"""Subagent registry — declarative definitions loaded from code and config."""

from __future__ import annotations

import json
import logging
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

logger = logging.getLogger(__name__)

SUBAGENTS_CONFIG_DIR = Path(__file__).resolve().parent.parent.parent / "configs" / "subagents"


@dataclass
class SubagentDefinition:
    """Declarative definition of a subagent."""

    name: str
    description: str
    system_prompt: str

    tool_allowlist: set[str] = field(default_factory=set)
    """Tool names the subagent is allowed to call. Empty = all tools available to the main agent."""

    model_override: str | None = None
    """Override the model used by this subagent (e.g. 'fast' for cheap models)."""

    max_iterations: int = 60
    context_warn_tokens: int = 170_000
    context_max_tokens: int = 190_000

    def to_tool_spec(self) -> dict[str, Any]:
        """Produce an LLM-facing tool spec for this subagent."""
        return {
            "name": self.name,
            "description": self.description,
            "parameters": {
                "type": "object",
                "properties": {
                    "task": {
                        "type": "string",
                        "description": (
                            "Detailed description of what you want this subagent to do. "
                            "Be specific about the task, context, and expected output format."
                        ),
                    },
                    "context": {
                        "type": "string",
                        "description": (
                            "Optional context from the current conversation that the "
                            "subagent needs to understand the task."
                        ),
                    },
                },
                "required": ["task"],
            },
        }


class SubagentRegistry:
    """Registry of available subagents.

    Built-in subagents are registered in code. Custom subagents
    can be loaded from YAML/JSON files in ``configs/subagents/``.
    """

    def __init__(self) -> None:
        self._definitions: dict[str, SubagentDefinition] = {}

    # ------------------------------------------------------------------
    # Registration
    # ------------------------------------------------------------------

    def register(self, definition: SubagentDefinition) -> None:
        if definition.name in self._definitions:
            logger.warning("Overwriting existing subagent definition: %s", definition.name)
        self._definitions[definition.name] = definition

    def register_builtins(self) -> None:
        """Register the built-in subagents shipped with the agent."""
        for builtin in _BUILTIN_SUBAGENTS:
            self.register(builtin)

    def load_from_config_dir(self) -> None:
        """Load custom subagent definitions from ``configs/subagents/``."""
        if not SUBAGENTS_CONFIG_DIR.is_dir():
            logger.debug("Subagent config dir not found: %s", SUBAGENTS_CONFIG_DIR)
            return

        for fpath in sorted(SUBAGENTS_CONFIG_DIR.iterdir()):
            if fpath.suffix not in (".json", ".yaml", ".yml"):
                continue
            try:
                text = fpath.read_text(encoding="utf-8")
                if fpath.suffix == ".json":
                    data = json.loads(text)
                else:
                    import yaml

                    data = yaml.safe_load(text)

                definitions = data if isinstance(data, list) else [data]
                for entry in definitions:
                    definition = SubagentDefinition(
                        name=entry["name"],
                        description=entry["description"],
                        system_prompt=entry["system_prompt"],
                        tool_allowlist=set(entry.get("tool_allowlist", [])),
                        model_override=entry.get("model_override"),
                        max_iterations=entry.get("max_iterations", 60),
                        context_warn_tokens=entry.get("context_warn_tokens", 170_000),
                        context_max_tokens=entry.get("context_max_tokens", 190_000),
                    )
                    self.register(definition)
                    logger.info("Loaded subagent from %s: %s", fpath.name, definition.name)
            except Exception:
                logger.exception("Failed to load subagent config: %s", fpath)

    # ------------------------------------------------------------------
    # Query
    # ------------------------------------------------------------------

    def get(self, name: str) -> SubagentDefinition | None:
        return self._definitions.get(name)

    def list_tool_specs(self) -> list[dict[str, Any]]:
        """Return tool specs for all registered subagents."""
        specs = []
        for defn in self._definitions.values():
            specs.append(defn.to_tool_spec())
        return specs

    def __contains__(self, name: str) -> bool:
        return name in self._definitions

    def __len__(self) -> int:
        return len(self._definitions)


# ======================================================================
# Built-in subagents
# ======================================================================

_CODEBASE_INVESTIGATOR_PROMPT = """\
You are a codebase investigation sub-agent. Your job is to deeply analyze
code, understand architecture, map dependencies, and answer detailed
questions about the codebase. You work autonomously with your own context.

# How to work

1. Use `read` to explore files and understand their structure.
2. Use `grep` and `glob` to find relevant patterns across the codebase.
3. Use `bash` to run linting, type-checking, or tests when needed.
4. Trace call chains and data flow across files.
5. Produce structured analysis in your final response.

# Output format

Return a structured analysis with:
- **Overview**: What the codebase or component does at a high level.
- **Architecture**: Key modules, files, classes, and how they connect.
- **Findings**: Answers to the specific questions asked.
- **Code references**: File paths and line numbers for important code.
- **Recommendations**: If appropriate, suggest improvements or next steps.

Be thorough but concise. Your output goes into the main agent's context —
focus on actionable information.
"""

_GENERALIST_PROMPT = """\
You are a generalist sub-agent. Your job is to execute a focused task
independently and return the result. You have access to a broad set of
tools and can research, write code, and run commands.

# How to work

1. Understand the task and break it into steps.
2. Research if needed (web search, docs, code).
3. Implement the solution.
4. Test or verify your work.
5. Return a summary of what was done and the result.

# Constraints

- You do NOT have access to other subagents — you cannot delegate further.
- You do NOT have access to notification or planning tools.
- You must stay within your context budget. Start wrapping up when warned.
- Your final message should be a complete summary of results.
"""

_BUILTIN_SUBAGENTS = [
    SubagentDefinition(
        name="codebase_investigator",
        description=(
            "Spawn a code-analysis sub-agent that deeply explores the codebase. "
            "Use for reverse-engineering, dependency mapping, finding where things "
            "are defined, or understanding complex code flow. The sub-agent can read "
            "files, grep, glob, and run commands."
        ),
        system_prompt=_CODEBASE_INVESTIGATOR_PROMPT,
        tool_allowlist={"read", "grep", "glob", "bash", "edit", "write"},
        max_iterations=40,
        context_warn_tokens=120_000,
        context_max_tokens=140_000,
    ),
    SubagentDefinition(
        name="subagent_generalist",
        description=(
            "Spawn a general-purpose sub-agent for any focused task that benefits "
            "from its own context window. Use for complex multi-step work where "
            "the main agent's context would be polluted by intermediate outputs. "
            "Has access to most tools including code editing, reading, search, and bash."
        ),
        system_prompt=_GENERALIST_PROMPT,
        max_iterations=60,
        context_warn_tokens=150_000,
        context_max_tokens=180_000,
    ),
]
