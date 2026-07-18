"""Regression tests for interactive CLI rendering and research model routing."""

import asyncio
import sys
from io import StringIO
from types import SimpleNamespace

import pytest
from rich.console import Console

import agent.main as main_mod
from agent.tools.research_tool import _get_research_model
from agent.utils import terminal_display


def test_router_anthropic_research_model_is_unchanged():
    assert (
        _get_research_model("anthropic/claude-opus-4.8:fal-ai")
        == "anthropic/claude-opus-4.8:fal-ai"
    )


def test_non_anthropic_research_model_is_unchanged():
    assert _get_research_model("openai/gpt-oss-120b") == "openai/gpt-oss-120b"


def test_sentinel_ai_prefix_research_model_strips_prefix():
    assert (
        _get_research_model("sentinel-ai/anthropic/claude-opus-4.8:fal-ai")
        == "anthropic/claude-opus-4.8:fal-ai"
    )


def test_help_output_keeps_descriptions_aligned(monkeypatch):
    output = StringIO()
    console = Console(
        file=output,
        color_system=None,
        theme=terminal_display._THEME,
        width=120,
    )
    monkeypatch.setattr(terminal_display, "_console", console)

    terminal_display.print_help()

    lines = [line.rstrip() for line in output.getvalue().splitlines() if line.strip()]
    description_columns = []
    for command, args, description in terminal_display.HELP_ROWS:
        line = next(line for line in lines if command in line)
        if args:
            assert args in line
        description_columns.append(line.index(description))

    assert len(set(description_columns)) == 1


def test_help_output_recomputes_widths_from_rows():
    rows = terminal_display.HELP_ROWS + (
        ("/longer-command", "[longer-args]", "Synthetic help row"),
    )
    output = StringIO()
    Console(
        file=output,
        color_system=None,
        theme=terminal_display._THEME,
        width=140,
    ).print(terminal_display.format_help_text(rows))

    lines = [line.rstrip() for line in output.getvalue().splitlines() if line.strip()]
    description_columns = [
        next(line for line in lines if command in line).index(description)
        for command, _args, description in rows
    ]

    assert len(set(description_columns)) == 1


def test_subagent_display_does_not_spawn_background_redraw(monkeypatch):
    calls: list[object] = []

    def _unexpected_future(*args, **kwargs):
        calls.append((args, kwargs))
        raise AssertionError("background redraw task should not be created")

    monkeypatch.setattr("asyncio.ensure_future", _unexpected_future)
    monkeypatch.setattr(
        terminal_display,
        "_console",
        SimpleNamespace(file=StringIO(), width=100),
    )

    mgr = terminal_display.SubAgentDisplayManager()
    mgr.start("agent-1", "research")
    mgr.add_call("agent-1", '▸ research  {"query": "search term"}')
    mgr.clear("agent-1")

    assert calls == []


def test_cli_forwards_model_flag_to_interactive_main(monkeypatch):
    seen: dict[str, object] = {}

    async def fake_main(*, model=None, sandbox_tools=False):
        seen["model"] = model
        seen["sandbox_tools"] = sandbox_tools

    monkeypatch.setattr(sys, "argv", ["sentinel-ai", "--model", "openai/gpt-5.5:fal-ai"])
    monkeypatch.setattr(main_mod, "main", fake_main)

    main_mod.cli()

    assert seen["model"] == "openai/gpt-5.5:fal-ai"
    assert seen["sandbox_tools"] is False


def test_cli_forwards_sandbox_flag_to_interactive_main(monkeypatch):
    seen: dict[str, object] = {}

    async def fake_main(*, model=None, sandbox_tools=False):
        seen["model"] = model
        seen["sandbox_tools"] = sandbox_tools

    monkeypatch.setattr(sys, "argv", ["sentinel-ai", "--sandbox-tools"])
    monkeypatch.setattr(main_mod, "main", fake_main)

    main_mod.cli()

    assert seen == {"model": None, "sandbox_tools": True}


def test_cli_forwards_sandbox_flag_to_headless_main(monkeypatch):
    seen: dict[str, object] = {}

    async def fake_headless_main(
        prompt,
        *,
        model=None,
        max_iterations=None,
        stream=True,
        sandbox_tools=False,
    ):
        seen.update(
            {
                "prompt": prompt,
                "model": model,
                "max_iterations": max_iterations,
                "stream": stream,
                "sandbox_tools": sandbox_tools,
            }
        )

    monkeypatch.setattr(
        sys,
        "argv",
        ["sentinel-ai", "--sandbox-tools", "--no-stream", "train a model"],
    )
    monkeypatch.setattr(main_mod, "headless_main", fake_headless_main)

    main_mod.cli()

    assert seen == {
        "prompt": "train a model",
        "model": None,
        "max_iterations": None,
        "stream": False,
        "sandbox_tools": True,
    }


@pytest.mark.asyncio
async def test_interactive_main_applies_model_override_before_banner(monkeypatch):
    class StopAfterBanner(Exception):
        pass

    def fake_banner(*, model=None, tool_runtime=None):
        assert model == "openai/gpt-5.5:fal-ai"
        assert tool_runtime == "local filesystem"
        raise StopAfterBanner

    monkeypatch.setattr(main_mod.os, "system", lambda *_args, **_kwargs: 0)
    monkeypatch.setattr(main_mod, "PromptSession", lambda: object())
    monkeypatch.setattr(
        main_mod,
        "load_config",
        lambda _path, **_kwargs: SimpleNamespace(
            model_name="moonshotai/Kimi-K2.7-Code",
            mcpServers={},
            tool_runtime="local",
        ),
    )
    monkeypatch.setattr(main_mod, "print_banner", fake_banner)

    with pytest.raises(StopAfterBanner):
        await main_mod.main(model="openai/gpt-5.5:fal-ai")


@pytest.mark.asyncio
async def test_initial_sandbox_preload_waits_before_prompt():
    waited = False

    async def preload():
        nonlocal waited
        await asyncio.sleep(0)
        waited = True

    task = asyncio.create_task(preload())
    await main_mod._wait_for_initial_sandbox_preload(
        [SimpleNamespace(sandbox_preload_task=task)]
    )

    assert waited is True
