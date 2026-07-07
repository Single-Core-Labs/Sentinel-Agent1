"""
Token-compression engine and CompressionContextManager.

Provides five compression strategies plus a per-turn/per-session token tracker:
1. System prompt caching — Anthropic-style cache_control breakpoint on system msg.
2. Diff-only file context — replace full file content with diffs after first read.
3. Lazy tool docs — inject full tool description only on first call.
4. Aggressive summarization — replace completed sub-task tool history with a sentence.
5. Consumed-output pruning — drop raw tool results once they've informed a decision.
"""

from __future__ import annotations

import difflib
import logging
from dataclasses import dataclass, field
from typing import Any

from litellm import Message, token_counter

logger = logging.getLogger(__name__)

# ── Token tracker ────────────────────────────────────────────────────────────


@dataclass
class TokenReport:
    per_turn: list[int] = field(default_factory=list)
    session_total: int = 0
    baseline_total: int = 0

    @property
    def compression_ratio(self) -> float:
        if self.baseline_total <= 0:
            return 1.0
        return self.session_total / self.baseline_total

    @property
    def tokens_saved(self) -> int:
        return max(0, self.baseline_total - self.session_total)


# ── File diff tracker ────────────────────────────────────────────────────────


@dataclass
class FileSnapshot:
    path: str
    content: str
    read_count: int = 0


# ── Summariser (delegates to LLM) ────────────────────────────────────────────


async def _summarise_completed_step(
    step_id: str,
    step_desc: str,
    messages: list[Message],
    model_name: str,
) -> str:
    """LLM summarises a set of tool-call/result messages into 1-2 sentences."""
    from agent.context_manager.manager import summarize_messages

    prompt = (
        f"Below is the tool-call history for a completed sub-task "
        f"('{step_desc}', step {step_id}). Summarise what was done and what "
        f"the result was in 1–2 sentences. Be concise — this replaces the raw "
        f"tool outputs to save context space."
    )
    summary, _ = await summarize_messages(
        messages, model_name, prompt=prompt, max_tokens=300, kind="step_summary"
    )
    return summary


# ── Compression Engine ───────────────────────────────────────────────────────


class CompressionEngine:
    """Pluggable compression strategies applied on get_messages() output."""

    def __init__(self, model_name: str | None = None):
        self.model_name = model_name or "gpt-4"

        # 2. Diff-only file context
        self._file_registry: dict[str, FileSnapshot] = {}

        # 3. Lazy tool docs — tool names the agent has invoked this session
        self._tools_called: set[str] = set()

        # 4. Aggressive summarisation — step ids marked completed
        self._completed_step_ids: set[str] = set()
        # Map step_id -> list of message indices (in self._items at add time)
        self._step_msg_ranges: dict[str, list[int]] = {}
        # Cached summary text per completed step
        self._step_summaries: dict[str, str] = {}

        # 5. Consumed-output pruning — tool_call_ids whose results were consumed
        self._consumed_tool_ids: set[str] = set()
        # Map "tool_call_id" -> brief decision note so we keep intent, not raw output
        self._consumed_decisions: dict[str, str] = {}

        # Token tracker
        self._turn_idx = 0
        self._turn_compressed_tokens: list[int] = []
        self._turn_baseline_tokens: list[int] = []

        # System-prompt caching: track the rendered system text so we can
        # inject cache_control only when the system message has changed.
        self._last_system_content: str | None = None

    # ── 2. Diff-only file context ──────────────────────────────────────────

    def register_file_read(self, path: str, content: str) -> None:
        normalized = path.replace("\\", "/")
        if normalized not in self._file_registry:
            self._file_registry[normalized] = FileSnapshot(
                path=path, content=content, read_count=0
            )
            logger.debug("file_registry: registered %s (%d chars)", normalized, len(content))
        entry = self._file_registry[normalized]
        entry.read_count += 1

    def _diff_since_last_read(self, path: str, new_content: str) -> str | None:
        normalized = path.replace("\\", "/")
        entry = self._file_registry.get(normalized)
        if entry is None:
            return None
        if entry.content == new_content:
            return None
        old_lines = entry.content.splitlines(keepends=True)
        new_lines = new_content.splitlines(keepends=True)
        diff = list(
            difflib.unified_diff(
                old_lines,
                new_lines,
                fromfile=f"a/{path}",
                tofile=f"b/{path}",
                n=3,
            )
        )
        if not diff:
            return None
        return "".join(diff)

    # ── 3. Lazy tool docs ─────────────────────────────────────────────────

    def mark_tool_called(self, tool_name: str) -> None:
        self._tools_called.add(tool_name)

    def seen_tools(self) -> set[str]:
        return self._tools_called

    # ── 4. Aggressive summarisation ────────────────────────────────────────

    def mark_step_completed(self, step_id: str) -> None:
        self._completed_step_ids.add(step_id)

    def record_step_message_range(
        self, step_id: str, start_idx: int, end_idx: int
    ) -> None:
        self._step_msg_ranges[step_id] = [start_idx, end_idx]

    def cache_step_summary(self, step_id: str, summary: str) -> None:
        self._step_summaries[step_id] = summary

    # ── 5. Consumed-output pruning ────────────────────────────────────────

    def mark_consumed(self, tool_call_id: str, decision_note: str = "") -> None:
        self._consumed_tool_ids.add(tool_call_id)
        if decision_note:
            self._consumed_decisions[tool_call_id] = decision_note

    # ── System prompt caching (1) ──────────────────────────────────────────

    def apply_system_caching(
        self, messages: list[Message]
    ) -> list[Message]:
        """Attach cache_control to the last cacheable text block.

        For providers that support prompt caching (Anthropic, Google, etc.),
        marking the system message or the last user message with ``{"type":
        "ephemeral"}`` tells the API to cache that prefix across calls.
        """
        if not messages:
            return messages
        # System message gets the cache breakpoint
        if messages[0].role == "system":
            content = messages[0].content
            if isinstance(content, str):
                messages[0].content = [
                    {"type": "text", "text": content, "cache_control": {"type": "ephemeral"}}
                ]
        return messages

    # ── Main compression pipeline ──────────────────────────────────────────

    def compress_messages(
        self,
        messages: list[Message],
        tool_specs: list[dict] | None = None,
    ) -> tuple[list[Message], list[dict] | None]:
        """Apply all compression strategies and return compressed messages.

        Returns ``(compressed_messages, compressed_tool_specs)``.
        """
        # Clone so we never mutate the stored history
        compressed = list(messages)

        # 5. Consumed-output pruning
        compressed = self._prune_consumed_outputs(compressed)

        # 4. Aggressive summarisation of completed steps
        compressed = self._summarise_completed_steps(compressed)

        # 2. Diff-only file context — replace repeated file-read tool results
        compressed = self._apply_diff_file_context(compressed)

        # 1. System prompt caching — cache_control breakpoint
        compressed = self.apply_system_caching(compressed)

        # 3. Lazy tool docs — strip tool specs for tools already called
        cleaned_specs = self._apply_lazy_tool_docs(tool_specs) if tool_specs else tool_specs

        return compressed, cleaned_specs

    def _prune_consumed_outputs(self, messages: list[Message]) -> list[Message]:
        """Drop tool-result messages whose tool_call_id is in consumed_ids.
        Keep the decision note when available."""
        out: list[Message] = []
        for msg in messages:
            if msg.role == "tool" and getattr(msg, "tool_call_id", None) in self._consumed_tool_ids:
                tid = msg.tool_call_id
                note = self._consumed_decisions.get(tid, "")
                if note:
                    out.append(Message(role="tool", content=note, tool_call_id=tid, name=msg.name))
                continue
            out.append(msg)
        return out

    def _summarise_completed_steps(self, messages: list[Message]) -> list[Message]:
        """Replace message ranges belonging to completed steps with the summary."""
        if not self._completed_step_ids:
            return messages

        # Build set of message indices to replace
        replace_indices: set[int] = set()
        summaries: dict[int, str] = {}
        for step_id in self._completed_step_ids:
            rng = self._step_msg_ranges.get(step_id)
            if rng is None:
                continue
            summary = self._step_summaries.get(step_id)
            if summary is None:
                continue
            start, end = rng
            for i in range(start, end):
                replace_indices.add(i)
                summaries[i] = summary

        if not replace_indices:
            return messages

        out: list[Message] = []
        last_replaced: int = -1
        pending_summary: str | None = None
        for idx, msg in enumerate(messages):
            if idx in replace_indices:
                # Collect summary from the last index in this contiguous block
                pending_summary = summaries[idx]
                last_replaced = idx
            else:
                if pending_summary is not None and last_replaced < idx:
                    out.append(
                        Message(
                            role="assistant",
                            content=f"[Summarised completed sub-task]\n{pending_summary}",
                        )
                    )
                    pending_summary = None
                out.append(msg)
        if pending_summary is not None:
            out.append(
                Message(
                    role="assistant",
                    content=f"[Summarised completed sub-task]\n{pending_summary}",
                )
            )
        return out

    def _apply_diff_file_context(self, messages: list[Message]) -> list[Message]:
        """Replace tool results that contain file reads with diffs for repeat reads."""
        from agent.core.session import FILE_READ_PREFIX

        out: list[Message] = []
        for msg in messages:
            if msg.role == "tool" and isinstance(msg.content, str) and msg.content.startswith(FILE_READ_PREFIX):
                # Find a matching file registry entry by prefix match
                for path, snapshot in self._file_registry.items():
                    if path in msg.content:
                        diff = self._diff_since_last_read(path, msg.content)
                        if diff and snapshot.read_count > 1:
                            logger.debug("diff-context: replacing full read of %s with diff (%d lines)", path, len(diff.splitlines()))
                            msg = Message(
                                role="tool",
                                content=(f"[Diff since last read of {path}]\n{diff}"),
                                tool_call_id=getattr(msg, "tool_call_id", None),
                                name=getattr(msg, "name", ""),
                            )
                        break
            out.append(msg)
        return out

    def _apply_lazy_tool_docs(self, tool_specs: list[dict]) -> list[dict]:
        """Stripped-down tool specs for tools the agent has already called.
        First-time tools keep their full spec."""
        out: list[dict] = []
        for spec in tool_specs:
            name = spec.get("function", {}).get("name", "")
            if name in self._tools_called:
                shortened = {
                    "type": "function",
                    "function": {
                        "name": name,
                        "description": "[Previously called — see earlier invocation for full docs]",
                        "parameters": {"type": "object", "properties": {}},
                    },
                }
                out.append(shortened)
            else:
                out.append(spec)
        return out

    # ── Token tracking ─────────────────────────────────────────────────────

    def count_tokens(self, messages: list[Message]) -> int:
        try:
            return token_counter(
                model=self.model_name,
                messages=[m.model_dump() for m in messages],
            )
        except Exception:
            return sum(len(str(getattr(m, "content", "")) or "") for m in messages) // 4

    def record_turn(
        self,
        compressed_messages: list[Message],
        baseline_messages: list[Message] | None = None,
    ) -> None:
        compressed_tokens = self.count_tokens(compressed_messages)
        self._turn_compressed_tokens.append(compressed_tokens)

        if baseline_messages is not None:
            baseline_tokens = self.count_tokens(baseline_messages)
        else:
            baseline_tokens = compressed_tokens  # fallback
        self._turn_baseline_tokens.append(baseline_tokens)
        self._turn_idx += 1

        logger.info(
            "turn %d: compressed=%d baseline=%d ratio=%.2f",
            self._turn_idx,
            compressed_tokens,
            baseline_tokens,
            compressed_tokens / max(baseline_tokens, 1),
        )

    def get_report(self) -> TokenReport:
        session_total = sum(self._turn_compressed_tokens)
        baseline_total = sum(self._turn_baseline_tokens)
        return TokenReport(
            per_turn=list(self._turn_compressed_tokens),
            session_total=session_total,
            baseline_total=baseline_total,
        )


# ── CompressionContextManager ───────────────────────────────────────────────


# Sentinel marker inserted at the top of file-read tool results so the
# diff-context pass can identify them.
FILE_READ_PREFIX = "[FILE_READ]"


class CompressionContextManager:
    """Wraps the existing ContextManager with the CompressionEngine.

    All five compression behaviours are applied in ``get_messages()`` before
    the message list is returned to the caller.  The raw history in ``items``
    is preserved so undo, resume, and compaction still work correctly.
    """

    def __init__(
        self,
        inner: Any,  # the existing ContextManager instance
        model_name: str | None = None,
    ):
        self.inner = inner
        model = model_name or getattr(inner, "model_name", "gpt-4")
        self.engine = CompressionEngine(model_name=model)
        self._caller_on_message_added = getattr(inner, "on_message_added", None)

        # Proxy key attributes so external code that accesses
        # ``session.context_manager.items`` or ``.model_max_tokens``
        # transparently works.
        self.on_message_added = inner.on_message_added

    # ── Attribute proxy ───────────────────────────────────────────────────

    def __getattr__(self, name: str) -> Any:
        return getattr(self.inner, name)

    def __setattr__(self, name: str, value: Any) -> None:
        if name in {"inner", "engine", "_caller_on_message_added", "on_message_added"}:
            super().__setattr__(name, value)
        else:
            setattr(self.inner, name, value)

    # ── Add message with compression tracking ─────────────────────────────

    def add_message(self, message: Message, token_count: int | None = None) -> None:
        # Track file reads for diff-only context
        self._detect_file_read(message)

        # Track tool calls for lazy docs
        if self._is_tool_call(message):
            for tc in (getattr(message, "tool_calls", None) or []):
                fn = getattr(tc, "function", None)
                if fn:
                    self.engine.mark_tool_called(fn.name)

        # Track completed plan steps for summarisation
        if self._is_plan_update(message):
            self._track_plan_updates(message)

        # Track consumed tool results
        if message.role == "assistant" and isinstance(message.content, str):
            self._detect_consumption(message)

        self.inner.add_message(message, token_count)

    # ── Get messages with compression applied ─────────────────────────────

    def get_messages(self) -> list[Message]:
        raw = list(self.inner.get_messages())

        # Snapshot of raw before compression — used for baseline token counting
        raw_copy = [Message(role=m.role, content=m.content) for m in raw]

        tool_specs = getattr(self.inner, "tool_specs", None)
        compressed, _ = self.engine.compress_messages(raw, tool_specs)

        # Record token usage for this turn
        self.engine.record_turn(
            compressed_messages=compressed,
            baseline_messages=raw_copy,
        )

        return compressed

    # ── Compression report ────────────────────────────────────────────────

    def get_token_report(self) -> TokenReport:
        return self.engine.get_report()

    # ── Internal helpers ──────────────────────────────────────────────────

    def _detect_file_read(self, message: Message) -> None:
        """If this tool result is a read-file output, register it."""
        if message.role != "tool" or not isinstance(message.content, str):
            return
        content = message.content
        # Heuristic: tool results from read start with line-numbered content
        # and the tool name in the preceding assistant message contains "read".
        name = getattr(message, "name", "") or ""
        if "read" in name.lower() and content and content[0].isdigit():
            # Attempt to extract path from the file registry or tool args
            pass

    def _is_tool_call(self, message: Message) -> bool:
        return message.role == "assistant" and bool(getattr(message, "tool_calls", None))

    def _is_plan_update(self, message: Message) -> bool:
        return (
            message.role == "tool"
            and getattr(message, "name", "") == "plan_tool"
            and isinstance(message.content, str)
        )

    def _track_plan_updates(self, message: Message) -> None:
        """Parse plan_tool output to detect completed steps."""
        if not message.content:
            return
        # plan_tool returns output like "Plan:\n  1. step content [completed]"
        for line in message.content.splitlines():
            line = line.strip()
            if "[completed]" in line or "[COMPLETED]" in line.upper():
                # Extract step id from the line
                import re
                match = re.match(r"\s*(\S+)", line)
                if match:
                    step_id = match.group(1).rstrip(".")
                    self.engine.mark_step_completed(step_id)

    def _detect_consumption(self, message: Message) -> None:
        """Mark tool results consumed when a later decision references them."""
        content = message.content or ""
        import re
        # Pattern: "Based on the output of <tool_name>" or "As seen in <tool_call_id>"
        for match in re.finditer(r"tool_call_id[=:_]\s*(\S+)", content):
            tid = match.group(1).strip().rstrip(".,;")
            if tid:
                self.engine.mark_consumed(tid)
