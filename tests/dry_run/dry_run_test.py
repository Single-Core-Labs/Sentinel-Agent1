#!/usr/bin/env python
"""
Dry-run: Core agent mechanics without platform-engineering dependencies.

Measures:
  1. Token-usage savings: compression ON vs OFF (core marketing claim)
  2. Doom-loop detector: does it fire on realistic debugging traces?
  3. Approval gate: does it block mutating operations?

Run:  uv run python tests/dry_run/dry_run_test.py
"""

from __future__ import annotations

import logging
import sys
from pathlib import Path

PROJECT_ROOT = Path(__file__).resolve().parent.parent.parent
if str(PROJECT_ROOT) not in sys.path:
    sys.path.insert(0, str(PROJECT_ROOT))

logging.basicConfig(
    level=logging.WARNING,
    format="%(levelname)-5s %(name)s %(message)s",
    stream=sys.stderr,
)
for name in ("LiteLLM", "LiteLLM Router", "openai", "httpx", "httpcore", "agent"):
    logging.getLogger(name).setLevel(logging.CRITICAL)


from litellm import Message, token_counter  # noqa: E402
from agent.context_manager.compression import CompressionEngine  # noqa: E402


from litellm.types.utils import Function as LiteLLMFunction, ChatCompletionMessageToolCall as LiteLLMToolCall  # noqa: E402


def _msg(role: str, content: str = "", tool_calls: list | None = None, **kw) -> Message:
    kwargs = dict(kw)
    if content:
        kwargs["content"] = content
    if tool_calls:
        kwargs["tool_calls"] = tool_calls
    return Message(role=role, **kwargs)


def _tool_call(name: str, args: dict, call_id: str = "call_1") -> LiteLLMToolCall:
    return LiteLLMToolCall(
        id=call_id,
        function=LiteLLMFunction(name=name, arguments=str(args)),
        type="function",
    )


def _tool_result(name: str, content: str, call_id: str = "call_1") -> Message:
    return Message(
        role="tool",
        content=content,
        tool_call_id=call_id,
    )


def _build_trajectory() -> list[Message]:
    msgs = []
    msgs.append(_msg("user", "Investigate the high error rate and fix it."))
    msgs.append(_msg("assistant",
        "I'll investigate the error rate issue. Let me check the code and logs.",
        tool_calls=[
            _tool_call("web_search", {"query": "high error rate debugging python"}, "call_search"),
        ]))
    msgs.append(_tool_result("web_search",
        "Common causes: unhandled exceptions, resource leaks, database connection pooling issues"))
    msgs.append(_msg("assistant",
        "Let me look at the codebase to understand the issue.",
        tool_calls=[
            _tool_call("execute_local", {"command": "grep -r 'try:' --include='*.py' src/ | head -20"}, "call_grep"),
            _tool_call("read", {"filePath": "src/main.py"}, "call_read"),
        ]))
    msgs.append(_tool_result("call_grep",
        "src/main.py:    try:\nsrc/main.py:        except Exception as e:\nsrc/handler.py:    try:\nsrc/handler.py:        except (ValueError, KeyError):"))
    msgs.append(_tool_result("call_read", "def main():\n    # entry point\n    try:\n        handler.process()\n    except Exception as e:\n        logger.error(f'Failed: {e}')"))
    msgs.append(_msg("assistant",
        "I see the issue. The error handling is too broad and swallows important context. Let me fix it.",
        tool_calls=[
            _tool_call("edit", {"filePath": "src/main.py", "oldString": "    try:\n        handler.process()\n    except Exception as e:\n        logger.error(f'Failed: {e}')", "newString": "    try:\n        handler.process()\n    except ValueError as e:\n        logger.error(f'Validation error: {e}')\n        raise\n    except Exception as e:\n        logger.error(f'Failed: {e}', exc_info=True)\n        raise"}, "call_edit"),
        ]))
    msgs.append(_tool_result("call_edit", "Applied edit to src/main.py"))
    return msgs


def _compression_test():
    print("=" * 60)
    print("  PHASE 1: COMPRESSION TEST")
    print("=" * 60)
    engine = CompressionEngine(token_counter=lambda msgs: token_counter("gpt-4o", msgs))
    trajectory = _build_trajectory()
    original_tokens = token_counter("gpt-4o", trajectory)
    compressed = engine.compress_messages(trajectory)
    compressed_tokens = token_counter("gpt-4o", compressed)
    ratio = compressed_tokens / original_tokens if original_tokens else 1
    print(f"  Original messages:  {len(trajectory)}")
    print(f"  Compressed messages: {len(compressed)}")
    print(f"  Original tokens:     {original_tokens}")
    print(f"  Compressed tokens:   {compressed_tokens}")
    print(f"  Compression ratio:   {ratio:.1%}")
    assert ratio <= 1.0, "Compression must not increase token count"
    print("  PASS")
    print()


def _doom_loop_test():
    print("=" * 60)
    print("  PHASE 2: DOOM-LOOP DETECTION")
    print("=" * 60)
    from agent.core.doom_loop import DoomLoopDetector
    detector = DoomLoopDetector()
    for i in range(6):
        result = detector.detect("execute_local", {"command": "kubectl get pods"})
        if i < 3:
            assert not result, f"Doom loop should not trigger at iteration {i}"
        if i >= 3:
            assert result, f"Doom loop should trigger at iteration {i}"
    print("  6 repeated kubectl calls: doom loop detected at iteration 4")
    print("  PASS")
    print()


def _approval_gate_test():
    print("=" * 60)
    print("  PHASE 3: APPROVAL GATE TEST")
    print("=" * 60)
    from agent.core.tools import is_mutating_tool
    readonly = ["web_search", "execute_local", "read", "grep", "glob"]
    mutating = ["edit", "write", "bash", "git_commit"]
    for t in readonly:
        assert not is_mutating_tool(t), f"{t} should not be mutating"
    for t in mutating:
        assert is_mutating_tool(t), f"{t} should be mutating"
    print(f"  Read-only tools: {readonly}")
    print(f"  Mutating tools:  {mutating}")
    print("  Classification correct")
    print("  PASS")
    print()


def main():
    _compression_test()
    _doom_loop_test()
    _approval_gate_test()
    print("=" * 60)
    print("  ALL PHASES PASSED")
    print("=" * 60)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
