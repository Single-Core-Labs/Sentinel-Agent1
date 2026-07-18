"""Subagent runner — spawns an isolated agent loop with its own context."""

from __future__ import annotations

import json
import logging
import time
import uuid
from typing import Any

from litellm import Message, acompletion

from agent.core import telemetry
from agent.core.doom_loop import check_for_doom_loop
from agent.core.llm_params import _resolve_llm_params
from agent.core.model_ids import strip_sentinel_ai_model_prefix
from agent.core.prompt_caching import (
    router_session_id_for,
    with_prompt_cache_params,
    with_prompt_caching,
)
from agent.core.session import Event
from agent.core.yolo_budget import maybe_pause_yolo_after_spend
from agent.subagents.registry import SubagentDefinition

logger = logging.getLogger(__name__)

# Subagents are NOT allowed to invoke other subagents — prevents recursion.
_FORBIDDEN_TOOL_PREFIXES = {"subagent_", "research"}


async def run_subagent(
    definition: SubagentDefinition,
    task: str,
    context: str,
    *,
    session: Any,
    tool_call_id: str | None = None,
) -> tuple[str, bool]:
    """Execute a subagent in an independent LLM loop.

    Returns ``(output_text, success_bool)``, matching the tool handler contract.
    """
    # Build independent context
    messages: list[Message] = [
        Message(role="system", content=definition.system_prompt),
    ]
    user_content = f"Task: {task}"
    if context:
        user_content = f"Context from main agent: {context}\n\n{user_content}"
    messages.append(Message(role="user", content=user_content))

    # Resolve model
    main_model = session.config.model_name
    subagent_model = definition.model_override or _get_subagent_model(main_model)
    _pref = getattr(session.config, "reasoning_effort", None)
    _capped = "high" if _pref in ("max", "xhigh") else _pref
    llm_params = _resolve_llm_params(
        subagent_model,
        getattr(session, "hf_token", None),
        reasoning_effort=_capped,
    )
    llm_params = with_prompt_cache_params(
        llm_params,
        session_id=router_session_id_for(session),
    )

    # Filter tools based on allowlist
    all_tool_specs = session.tool_router.get_tool_specs_for_llm()
    tool_specs = _filter_tool_specs(all_tool_specs, definition.tool_allowlist)

    # Agent identification for UI
    _agent_id = tool_call_id or uuid.uuid4().hex[:8]
    _label = f"{definition.name}: " + (task[:50] + "\u2026" if len(task) > 50 else task)

    async def _log(text: str) -> None:
        try:
            await session.send_event(
                Event(
                    event_type="tool_log",
                    data={
                        "tool": definition.name,
                        "log": text,
                        "agent_id": _agent_id,
                        "label": _label,
                    },
                )
            )
        except Exception:
            pass

    _tool_uses = 0
    _total_tokens = 0
    _warned_context = False
    _max_iterations = definition.max_iterations
    _ctx_warn = definition.context_warn_tokens
    _ctx_max = definition.context_max_tokens

    await _log(f"Starting sub-agent ({definition.name})...")

    for _iteration in range(_max_iterations):
        # ── Doom-loop detection ──
        doom_prompt = check_for_doom_loop(messages)
        if doom_prompt:
            logger.warning(
                "Sub-agent %s doom-loop activated at iteration %d",
                definition.name,
                _iteration,
            )
            messages.append(Message(role="user", content=doom_prompt))

        # ── Context budget management ──
        if _total_tokens >= _ctx_max:
            logger.warning(
                "Sub-agent %s hit context max (%d tokens)",
                definition.name,
                _total_tokens,
            )
            await _log(f"Context limit reached ({_total_tokens} tokens) \u2014 forcing wrap-up")
            messages.append(
                Message(
                    role="user",
                    content=(
                        "[SYSTEM: CONTEXT LIMIT REACHED] You have used all available context. "
                        "Summarize your findings NOW. Do NOT call any more tools."
                    ),
                )
            )
            try:
                _t0 = time.monotonic()
                cached_messages, _ = with_prompt_caching(messages, None, llm_params)
                response = await _acompletion(
                    session=session,
                    model=subagent_model,
                    messages=cached_messages,
                    tools=None,
                    llm_params=llm_params,
                    timeout=120,
                )
                try:
                    if await _record_llm_call(session, subagent_model, response, _t0):
                        return "Sub-agent paused: budget cap reached.", False
                except Exception:
                    pass
                content = response.choices[0].message.content or ""
                return content or "Context exhausted \u2014 no summary.", bool(content)
            except Exception:
                return "Sub-agent context exhausted and summary call failed.", False

        if not _warned_context and _total_tokens >= _ctx_warn:
            _warned_context = True
            await _log(f"Context at {_total_tokens} tokens \u2014 nudging to wrap up")
            messages.append(
                Message(
                    role="user",
                    content=(
                        "[SYSTEM: You have used 75% of your context budget. "
                        "Start wrapping up: finish any critical lookups, then "
                        "produce your final summary within the next 1-2 iterations.]"
                    ),
                )
            )

        # ── LLM call ──
        try:
            _t0 = time.monotonic()
            cached_messages, cached_tools = with_prompt_caching(
                messages, tool_specs if tool_specs else None, llm_params
            )
            response = await _acompletion(
                session=session,
                model=subagent_model,
                messages=cached_messages,
                tools=cached_tools,
                tool_choice="auto",
                llm_params=llm_params,
                timeout=120,
            )
            try:
                if await _record_llm_call(session, subagent_model, response, _t0):
                    return "Sub-agent paused: budget cap reached.", False
            except Exception:
                pass
        except Exception as e:
            logger.error("Sub-agent %s LLM error: %s", definition.name, e)
            return f"Sub-agent '{definition.name}' LLM error: {e}", False

        # Track tokens
        if response.usage:
            _total_tokens = response.usage.total_tokens
            await _log(f"tokens:{_total_tokens}")

        choice = response.choices[0]
        msg = choice.message

        # No tool calls = final answer
        if not msg.tool_calls:
            await _log(f"{definition.name} complete.")
            content = msg.content or "Task completed but no summary generated."
            return content, True

        # Execute tool calls
        messages.append(
            Message(
                role="assistant",
                content=msg.content,
                tool_calls=msg.tool_calls,
            )
        )
        for tc in msg.tool_calls:
            try:
                tool_args = json.loads(tc.function.arguments)
            except (json.JSONDecodeError, TypeError):
                messages.append(
                    Message(
                        role="tool",
                        content="Invalid tool arguments.",
                        tool_call_id=tc.id,
                        name=tc.function.name,
                    )
                )
                continue

            tool_name = tc.function.name

            # Enforce recursion protection
            if _is_forbidden_tool(tool_name):
                messages.append(
                    Message(
                        role="tool",
                        content=(
                            f"Tool '{tool_name}' is not available to sub-agents "
                            "(sub-agents cannot invoke other sub-agents)."
                        ),
                        tool_call_id=tc.id,
                        name=tool_name,
                    )
                )
                continue

            # Enforce allowlist
            if definition.tool_allowlist and tool_name not in definition.tool_allowlist:
                messages.append(
                    Message(
                        role="tool",
                        content=(
                            f"Tool '{tool_name}' is not in this sub-agent's allowlist. "
                            f"Allowed tools: {', '.join(sorted(definition.tool_allowlist))}"
                        ),
                        tool_call_id=tc.id,
                        name=tool_name,
                    )
                )
                continue

            try:
                import json as _json

                args_str = _json.dumps(tool_args)[:80]
                await _log(f"\u25b8 {tool_name}  {args_str}")

                output, _success = await session.tool_router.call_tool(
                    tool_name, tool_args, session=session, tool_call_id=tc.id
                )
                _tool_uses += 1
                await _log(f"tools:{_tool_uses}")
                if len(output) > 8000:
                    output = output[:4800] + "\n...(truncated)...\n" + output[-3200:]
            except Exception as e:
                output = f"Tool error: {e}"

            messages.append(
                Message(
                    role="tool",
                    content=output,
                    tool_call_id=tc.id,
                    name=tool_name,
                )
            )

    # ── Iteration limit ──
    await _log("Iteration limit reached \u2014 extracting summary")
    messages.append(
        Message(
            role="user",
            content=(
                "[SYSTEM: ITERATION LIMIT] You have reached the maximum number of iterations. "
                "Summarize ALL findings so far. Do NOT call any more tools."
            ),
        )
    )
    try:
        _t0 = time.monotonic()
        cached_messages, _ = with_prompt_caching(messages, None, llm_params)
        response = await _acompletion(
            session=session,
            model=subagent_model,
            messages=cached_messages,
            tools=None,
            llm_params=llm_params,
            timeout=120,
        )
        try:
            if await _record_llm_call(session, subagent_model, response, _t0):
                return "Sub-agent paused: budget cap reached.", False
        except Exception:
            pass
        content = response.choices[0].message.content or ""
        if content:
            return content, True
    except Exception as e:
        logger.error("Sub-agent %s summary call failed: %s", definition.name, e)

    return (
        f"Sub-agent '{definition.name}' hit iteration limit ({_max_iterations}). "
        f"Try a more focused task.",
        False,
    )


# ======================================================================
# Internal helpers
# ======================================================================


def _is_forbidden_tool(tool_name: str) -> bool:
    """Check if a tool is a subagent or research tool (recursion guard)."""
    for prefix in _FORBIDDEN_TOOL_PREFIXES:
        if tool_name.startswith(prefix):
            return True
    return False


def _get_subagent_model(main_model: str) -> str:
    """Normalise the main model ID for subagent calls."""
    return strip_sentinel_ai_model_prefix(main_model) or main_model


def _filter_tool_specs(
    specs: list[dict[str, Any]],
    allowlist: set[str],
) -> list[dict[str, Any]]:
    """Filter tool specs to only those in the allowlist (if non-empty)."""
    if not allowlist:
        return specs
    return [
        s for s in specs if s["function"]["name"] in allowlist
    ]


async def _acompletion(
    *,
    session: Any,
    model: str,
    messages: list[Any],
    tools: Any,
    llm_params: dict[str, Any],
    timeout: int,
    tool_choice: str | None = None,
):
    kwargs: dict[str, Any] = {
        "messages": messages,
        "tools": tools,
        "stream": False,
        "timeout": timeout,
        **llm_params,
    }
    if tool_choice is not None:
        kwargs["tool_choice"] = tool_choice
    return await acompletion(**kwargs)


async def _record_llm_call(
    session: Any,
    model: str,
    response: Any,
    started_at: float,
) -> bool:
    usage = await telemetry.record_llm_call(
        session,
        model=model,
        response=response,
        latency_ms=int((time.monotonic() - started_at) * 1000),
        finish_reason=response.choices[0].finish_reason if response.choices else None,
        kind="subagent",
    )
    return await maybe_pause_yolo_after_spend(
        session,
        spend_kind="subagent",
        observed_cost_usd=usage.get("cost_usd") if isinstance(usage, dict) else None,
    )
