"""Smoke test for the SFT reshape — raw passthrough with tags attached."""

import importlib.util
import sys
from pathlib import Path


def _load():
    path = Path(__file__).parent.parent.parent / "scripts" / "build_sft.py"
    spec = importlib.util.spec_from_file_location("build_sft", path)
    mod = importlib.util.module_from_spec(spec)
    sys.modules["build_sft"] = mod
    spec.loader.exec_module(mod)  # type: ignore
    return mod


def _session_row():
    return {
        "session_id": "abc",
        "session_start_time": "2026-04-24T10:00:00",
        "session_end_time": "2026-04-24T10:05:00",
        "model_name": "anthropic/claude-opus-4.8:fal-ai",
        "messages": [
            {"role": "system", "content": "You are an agent"},
            {"role": "user", "content": "fine-tune llama"},
            {
                "role": "assistant",
                "content": None,
                "tool_calls": [
                    {
                        "id": "c1",
                        "type": "function",
                        "function": {
                            "name": "bash",
                            "arguments": '{"script":"from trl import SFTTrainer"}',
                        },
                    },
                ],
            },
            {"role": "tool", "tool_call_id": "c1", "content": "ok"},
            {"role": "assistant", "content": "done"},
        ],
        "events": [
            {
                "timestamp": "2026-04-24T10:00:05",
                "event_type": "tool_call",
                "data": {
                    "tool": "bash",
                    "arguments": {"script": "from trl import SFTTrainer"},
                },
            },
            {
                "timestamp": "2026-04-24T10:00:06",
                "event_type": "turn_complete",
                "data": {},
            },
        ],
        "tools": [{"type": "function", "function": {"name": "bash"}}],
    }


def test_reshape_preserves_messages_and_tools_and_adds_tags():
    mod = _load()
    row = mod._reshape_to_sft(_session_row())
    assert row["session_id"] == "abc"
    assert row["model"] == "anthropic/claude-opus-4.8:fal-ai"
    assert row["timestamp"] == "2026-04-24T10:00:00"
    # Messages preserved verbatim, in order, with tool_calls + tool role rows.
    assert len(row["messages"]) == 5
    assert row["messages"][2]["tool_calls"][0]["function"]["name"] == "bash"
    assert row["messages"][3]["role"] == "tool"
    # Tools preserved verbatim.
    assert row["tools"] == [{"type": "function", "function": {"name": "bash"}}]
    # Tags include the expected signals.
    tags = set(row["tags"])
    assert "tool:bash" in tags
    assert "outcome:completed" in tags
    assert "model:opus" in tags


def test_reshape_handles_missing_tools_field():
    mod = _load()
    row = _session_row()
    del row["tools"]
    out = mod._reshape_to_sft(row)
    assert out["tools"] == []
    assert isinstance(out["tags"], list)  # still computes tags
