"""Helper functions for agent_loop."""

import asyncio
import logging
import time
from dataclasses import dataclass, field
from typing import Any

from litellm import (
    ChatCompletionMessageToolCall,
    Message,
    acompletion,
)
from litellm.exceptions import ContextWindowExceededError

from agent.config import Config
from agent.core.approval_policy import (
    normalize_tool_operation,
)
from agent.core.cost_estimation import CostEstimate, estimate_tool_cost
from agent.core.llm_params import _resolve_llm_params
from agent.core.prompt_caching import (
    router_session_id_for,
    with_prompt_cache_params,
    with_prompt_caching,
)
from agent.core.session import (
    Event,
    Session,
)
from agent.core.yolo_budget import (
    BudgetDecision,
    check_session_budget,
    reserve_session_budget,
)


logger = logging.getLogger(__name__)

ToolCall = ChatCompletionMessageToolCall

_MALFORMED_TOOL_PREFIX = "ERROR: Tool call to '"
_MALFORMED_TOOL_SUFFIX = "' had malformed JSON arguments"
_NO_TOOL_INCOMPLETE_PLAN_RETRY_LIMIT = 2
# Hard cap per user turn: 50 iterations in plan phase, 200 total
_MAX_PLAN_ITERATIONS = 50
_MAX_TOTAL_ITERATIONS = 200


def _unfinished_plan_items(session: Session) -> list[dict[str, str]]:
    plan = getattr(session, "current_plan", None) or []
    unfinished: list[dict[str, str]] = []
    for item in plan:
        if not isinstance(item, dict):
            continue
        status = item.get("status")
        if status in {"pending", "in_progress"}:
            unfinished.append(item)
    return unfinished


def _format_plan_items_for_guard(items: list[dict[str, str]], limit: int = 4) -> str:
    formatted = []
    for item in items[:limit]:
        item_id = item.get("id") or "?"
        content = item.get("content") or "(unnamed task)"
        status = item.get("status") or "unknown"
        formatted.append(f"{item_id}. {content} [{status}]")
    if len(items) > limit:
        formatted.append(f"... and {len(items) - limit} more")
    return "; ".join(formatted)


def _no_tool_incomplete_plan_prompt(items: list[dict[str, str]]) -> str:
    summary = _format_plan_items_for_guard(items)
    return (
        "[SYSTEM: CONTINUATION GUARD] Your previous response ended without any "
        "tool calls, but the task is not complete. The current plan still has "
        f"unfinished items: {summary}. Do not return control to the user yet. "
        "Continue from the next unfinished item and make at least one tool call "
        "now. If you genuinely cannot continue, first use tools to inspect the "
        "state or verify the blocker."
    )


def _malformed_tool_name(message: Message) -> str | None:
    """Return the tool name for malformed-json tool-result messages."""
    if getattr(message, "role", None) != "tool":
        return None
    content = getattr(message, "content", None)
    if not isinstance(content, str):
        return None
    if not content.startswith(_MALFORMED_TOOL_PREFIX):
        return None
    end = content.find(_MALFORMED_TOOL_SUFFIX, len(_MALFORMED_TOOL_PREFIX))
    if end == -1:
        return None
    return content[len(_MALFORMED_TOOL_PREFIX) : end]


def _detect_repeated_malformed(
    items: list[Message],
    threshold: int = 2,
) -> str | None:
    """Return the repeated malformed tool name if the tail contains a streak.

    Walk backward over the current conversation tail. A streak counts only
    consecutive malformed tool-result messages for the same tool; any other
    tool result breaks it.
    """
    if threshold <= 0:
        return None

    streak_tool: str | None = None
    streak = 0

    for item in reversed(items):
        if getattr(item, "role", None) != "tool":
            continue

        malformed_tool = _malformed_tool_name(item)
        if malformed_tool is None:
            break

        if streak_tool is None:
            streak_tool = malformed_tool
            streak = 1
        elif malformed_tool == streak_tool:
            streak += 1
        else:
            break

        if streak >= threshold:
            return streak_tool

    return None


def _coerce_float(value: Any) -> float:
    if isinstance(value, bool) or value is None:
        return 0.0
    try:
        return float(value)
    except (TypeError, ValueError):
        return 0.0


def _usage_output_message(pending: dict[str, Any]) -> str:
    current = _coerce_float(pending.get("current_spend_usd"))
    next_threshold = _coerce_float(pending.get("next_threshold_usd"))
    return (
        f"Current-session usage warning acknowledged at ${current:.2f}. "
        f"The next warning is at ${next_threshold:.2f}."
    )


async def _maybe_pause_for_usage_threshold(
    session: Session,
    *,
    continuation: str,
    final_response: str | None = None,
) -> bool:
    checker = getattr(session, "usage_threshold_checker", None)
    if checker is None or session.pending_approval:
        return False
    payload: dict[str, Any] = {
        "continuation": continuation,
        "force_check": continuation == "complete_turn",
        "history_size": len(session.context_manager.items),
    }
    if final_response is not None:
        payload["final_response"] = final_response
    try:
        return bool(await checker(payload))
    except Exception as e:
        logger.debug("Usage threshold check failed: %s", e)
        return False


def _validate_tool_args(tool_args: dict) -> tuple[bool, str | None]:
    """
    Validate tool arguments structure.

    Returns:
        (is_valid, error_message)
    """
    args = tool_args.get("args", {})
    # Sometimes LLM passes args as string instead of dict
    if isinstance(args, str):
        return (
            False,
            f"Tool call error: 'args' must be a JSON object, not a string. You passed: {repr(args)}",
        )
    if not isinstance(args, dict) and args is not None:
        return (
            False,
            f"Tool call error: 'args' must be a JSON object. You passed type: {type(args).__name__}",
        )
    return True, None


@dataclass(frozen=True)
class ApprovalDecision:
    requires_approval: bool
    auto_approved: bool = False
    auto_approval_blocked: bool = False
    block_reason: str | None = None
    estimated_cost_usd: float | None = None
    remaining_cap_usd: float | None = None
    billable: bool = False


def _operation(tool_args: dict) -> str:
    return normalize_tool_operation(tool_args.get("operation"))


def _base_needs_approval(
    tool_name: str, tool_args: dict, config: Config | None = None
) -> bool:
    """Check if a tool call requires approval before YOLO policy is applied."""

    # If args are malformed, skip approval (validation error will be shown later)
    args_valid, _ = _validate_tool_args(tool_args)
    if not args_valid:
        return False

    return False


def _session_auto_approval_enabled(session: Session | None) -> bool:
    return bool(session and getattr(session, "auto_approval_enabled", False))


def _effective_yolo_enabled(session: Session | None, config: Config | None) -> bool:
    return bool(
        (config and config.yolo_mode) or _session_auto_approval_enabled(session)
    )


async def _approval_decision(
    tool_name: str,
    tool_args: dict,
    session: Session,
    *,
    reserved_spend_usd: float = 0.0,
) -> ApprovalDecision:
    """Return the approval decision for one parsed tool call."""
    config = session.config
    base_requires_approval = _base_needs_approval(tool_name, tool_args, config)

    yolo_enabled = _effective_yolo_enabled(session, config)
    budgeted_target = _is_budgeted_auto_approval_target(tool_name, tool_args)

    # Cost caps are a session-scoped web policy. Legacy config.yolo_mode
    # remains uncapped for CLI/headless, except for scheduled jobs above.
    session_yolo_enabled = _session_auto_approval_enabled(session)
    if yolo_enabled and budgeted_target and session_yolo_enabled:
        estimate = await estimate_tool_cost(tool_name, tool_args, session=session)
        budget = check_session_budget(
            session,
            estimate,
            reserved_spend_usd=reserved_spend_usd,
        )
        if not budget.allowed:
            return ApprovalDecision(
                requires_approval=True,
                auto_approval_blocked=True,
                block_reason=budget.block_reason,
                estimated_cost_usd=budget.estimated_cost_usd,
                remaining_cap_usd=budget.remaining_cap_usd,
                billable=estimate.billable,
            )
        if base_requires_approval:
            return ApprovalDecision(
                requires_approval=False,
                auto_approved=True,
                estimated_cost_usd=budget.estimated_cost_usd,
                remaining_cap_usd=budget.remaining_cap_usd,
                billable=estimate.billable,
            )
        return ApprovalDecision(
            requires_approval=False,
            estimated_cost_usd=budget.estimated_cost_usd,
            remaining_cap_usd=budget.remaining_cap_usd,
            billable=estimate.billable,
        )

    # ── Mandatory-approval tools: NO bypass, not even yolo mode ──
    if _mandatory_approval_tool(tool_name):
        return ApprovalDecision(requires_approval=True)

    if base_requires_approval and yolo_enabled:
        return ApprovalDecision(requires_approval=False, auto_approved=True)

    return ApprovalDecision(requires_approval=base_requires_approval)


def _record_estimated_spend(
    session: Session,
    decision: ApprovalDecision,
    *,
    reservation_id: str | None = None,
) -> BudgetDecision:
    if not decision.billable or decision.estimated_cost_usd is None:
        return BudgetDecision(allowed=True, billable=False)
    return reserve_session_budget(
        session,
        CostEstimate(
            estimated_cost_usd=decision.estimated_cost_usd,
            billable=True,
        ),
        spend_kind="tool",
        reservation_id=reservation_id,
    )


async def _record_manual_approved_spend_if_needed(
    session: Session,
    tool_name: str,
    tool_args: dict,
    *,
    tool_call_id: str | None = None,
) -> BudgetDecision:
    if not _session_auto_approval_enabled(session):
        return BudgetDecision(allowed=True)
    if not _is_budgeted_auto_approval_target(tool_name, tool_args):
        return BudgetDecision(allowed=True)
    estimate = await estimate_tool_cost(tool_name, tool_args, session=session)
    return reserve_session_budget(
        session,
        estimate,
        spend_kind=tool_name,
        reservation_id=tool_call_id,
    )


async def _check_manual_approved_budget(
    session: Session,
    tool_name: str,
    tool_args: dict,
    *,
    reserved_spend_usd: float = 0.0,
) -> BudgetDecision:
    if not _session_auto_approval_enabled(session):
        return BudgetDecision(allowed=True)
    if not _is_budgeted_auto_approval_target(tool_name, tool_args):
        return BudgetDecision(allowed=True)
    estimate = await estimate_tool_cost(tool_name, tool_args, session=session)
    return check_session_budget(
        session,
        estimate,
        reserved_spend_usd=reserved_spend_usd,
    )


# -- LLM retry constants --------------------------------------------------
_MAX_LLM_RETRIES = 3
_LLM_RETRY_DELAYS = [5, 15, 30]  # seconds between retries
_LLM_RATE_LIMIT_RETRY_DELAYS = [30, 60]


def _is_rate_limit_error(error: Exception) -> bool:
    """Return True for rate-limit / quota-bucket style provider errors."""
    err_str = str(error).lower()
    rate_limit_patterns = [
        "429",
        "rate limit",
        "rate_limit",
        "too many requests",
        "too many tokens",
        "request limit",
        "throttl",
    ]
    return any(pattern in err_str for pattern in rate_limit_patterns)


def _is_context_overflow_error(error: Exception) -> bool:
    """Return True when the prompt exceeded the model's context window."""
    if isinstance(error, ContextWindowExceededError):
        return True

    err_str = str(error).lower()
    overflow_patterns = [
        "context window exceeded",
        "maximum context length",
        "max context length",
        "prompt is too long",
        "context length exceeded",
        "too many input tokens",
        "input is too long",
    ]
    return any(pattern in err_str for pattern in overflow_patterns)


def _retry_delay_for(error: Exception, attempt_index: int) -> int | None:
    """Return the delay for this retry attempt, or None if it should not retry."""
    if _is_rate_limit_error(error):
        schedule = _LLM_RATE_LIMIT_RETRY_DELAYS
    elif _is_transient_error(error):
        schedule = _LLM_RETRY_DELAYS
    else:
        return None

    if attempt_index >= len(schedule):
        return None
    return schedule[attempt_index]


def _is_transient_error(error: Exception) -> bool:
    """Return True for errors that are likely transient and worth retrying."""
    err_str = str(error).lower()
    transient_patterns = [
        "timeout",
        "timed out",
        "503",
        "service unavailable",
        "502",
        "bad gateway",
        "500",
        "internal server error",
        "overloaded",
        "capacity",
        "connection reset",
        "connection refused",
        "connection error",
        "eof",
        "broken pipe",
    ]
    return _is_rate_limit_error(error) or any(
        pattern in err_str for pattern in transient_patterns
    )


def _is_effort_config_error(error: Exception) -> bool:
    """Catch the two 400s the effort probe also handles — thinking
    unsupported for this model, or the specific effort level invalid.

    This is our safety net for the case where ``/effort`` was changed
    mid-conversation (which clears the probe cache) and the new level
    doesn't work for the current model. We heal the cache and retry once.
    """
    from agent.core.effort_probe import _is_invalid_effort, _is_thinking_unsupported

    return _is_thinking_unsupported(error) or _is_invalid_effort(error)


async def _heal_effort_and_rebuild_params(
    session: Session,
    error: Exception,
    llm_params: dict,
) -> dict:
    from agent.core.effort_probe import (
        ProbeInconclusive,
        _is_thinking_unsupported,
        probe_effort,
    )

    model = session.config.model_name
    if _is_thinking_unsupported(error):
        session.model_effective_effort[model] = None
        logger.info("healed: %s doesn't support thinking — stripped", model)
    else:
        try:
            outcome = await probe_effort(
                model,
                session.config.reasoning_effort,
                session.hf_token,
                session=session,
            )
            session.model_effective_effort[model] = outcome.effective_effort
            logger.info(
                "healed: %s effort cascade → %s",
                model,
                outcome.effective_effort,
            )
        except ProbeInconclusive:
            session.model_effective_effort[model] = None
            logger.info("healed: %s probe inconclusive — stripped", model)

    return _resolve_llm_params(
        model,
        session.hf_token or session.provider_api_key,
        reasoning_effort=session.effective_effort_for(model),
        provider_api_key=session.provider_api_key,
        provider_id=session.provider_id,
    )


def _friendly_error_message(
    error: Exception,
    *,
    user_plan: str | None = None,
) -> str | None:
    """Return a user-friendly message for known error types, or None to fall back to traceback."""
    err_str = str(error).lower()

    if (
        "authentication" in err_str
        or "unauthorized" in err_str
        or "invalid x-api-key" in err_str
    ):
        return (
            "Authentication failed - your token is missing or invalid.\n\n"
            "To fix this, set the appropriate token or login.\n\n"
            "You can also add it to a .env file in the project root.\n"
            "To switch models, use the /model command."
        )

    if "model_not_found" in err_str or (
        "model" in err_str and ("not found" in err_str or "does not exist" in err_str)
    ):
        return (
            "Model not found. Use '/model' to list suggestions. "
            "Availability is shown when you switch."
        )

    return None


async def _compact_and_notify(session: Session) -> None:
    """Run compaction and send event if context was reduced.

    Catches ``CompactionFailedError`` and ends the session cleanly instead
    of letting the caller retry. Pre-2026-05-04 the caller looped on
    ContextWindowExceededError → compact → re-trigger, burning hosted
    inference budget while the session never reached the upload path.
    """
    from agent.context_manager.manager import CompactionFailedError

    cm = session.context_manager
    old_usage = cm.running_context_usage
    logger.debug(
        "Compaction check: usage=%d, max=%d, threshold=%d, needs_compact=%s",
        old_usage,
        cm.model_max_tokens,
        cm.compaction_threshold,
        cm.needs_compaction,
    )
    try:
        await cm.compact(
            model_name=session.config.model_name,
            tool_specs=session.tool_router.get_tool_specs_for_llm(),
            hf_token=session.hf_token,
            session=session,
        )
    except CompactionFailedError as e:
        logger.error(
            "Compaction failed for session %s: %s — terminating session",
            session.session_id,
            e,
        )
        # Persist the failure event so the dataset has a record of WHY this
        # session ended (and the cost it incurred up to that point) even if
        # save_and_upload_detached has issues downstream.
        await session.send_event(
            Event(
                event_type="session_terminated",
                data={
                    "reason": "compaction_failed",
                    "context_usage": cm.running_context_usage,
                    "context_threshold": cm.compaction_threshold,
                    "error": str(e)[:300],
                    "user_message": (
                        "Your conversation has grown too large to continue. "
                        "The work you've done is saved — start a new session to keep going."
                    ),
                },
            )
        )
        # Stop the agent loop; the finally in _run_session will fire
        # cleanup_sandbox + save_trajectory so the dataset captures
        # everything that did happen.
        session.is_running = False
        return

    new_usage = cm.running_context_usage
    if new_usage != old_usage:
        logger.warning(
            "Context compacted: %d -> %d tokens (max=%d, %d messages)",
            old_usage,
            new_usage,
            cm.model_max_tokens,
            len(cm.items),
        )
        await session.send_event(
            Event(
                event_type="compacted",
                data={"old_tokens": old_usage, "new_tokens": new_usage},
            )
        )





@dataclass
class LLMResult:
    """Result from an LLM call (streaming or non-streaming)."""

    content: str | None
    tool_calls_acc: dict[int, dict]
    token_count: int
    finish_reason: str | None
    usage: dict = field(default_factory=dict)


def _session_cancelled(session: Any) -> bool:
    return bool(getattr(session, "is_cancelled", False))


async def _sleep_for_retry_or_cancel(session: Session, delay: float) -> bool:
    """Sleep for a retry delay, waking early if the session is interrupted."""
    if _session_cancelled(session):
        return True

    cancel_event = getattr(session, "_cancelled", None)
    if cancel_event is None or not hasattr(cancel_event, "wait"):
        await asyncio.sleep(delay)
        return _session_cancelled(session)

    sleep_task = asyncio.create_task(asyncio.sleep(delay))
    cancel_task = asyncio.create_task(cancel_event.wait())
    done, pending = await asyncio.wait(
        {sleep_task, cancel_task},
        return_when=asyncio.FIRST_COMPLETED,
    )
    for task in pending:
        task.cancel()
    if pending:
        await asyncio.gather(*pending, return_exceptions=True)
    return cancel_task in done or _session_cancelled(session)


def _is_invalid_thinking_signature_error(exc: Exception) -> bool:
    """Return True when a provider rejected replayed thinking metadata."""
    text = str(exc)
    return (
        "Invalid `signature` in `thinking` block" in text
        or "Invalid signature in thinking block" in text
    )


def _strip_thinking_state_from_messages(messages: list[Any]) -> int:
    """Remove replayed thinking metadata from assistant history messages."""
    stripped = 0

    for message in messages:
        role = (
            message.get("role")
            if isinstance(message, dict)
            else getattr(message, "role", None)
        )
        if role != "assistant":
            continue

        if isinstance(message, dict):
            if message.pop("thinking_blocks", None) is not None:
                stripped += 1
            if message.pop("reasoning_content", None) is not None:
                stripped += 1
            provider_fields = message.get("provider_specific_fields")
            content = message.get("content")
        else:
            if getattr(message, "thinking_blocks", None) is not None:
                message.thinking_blocks = None
                stripped += 1
            if getattr(message, "reasoning_content", None) is not None:
                message.reasoning_content = None
                stripped += 1
            provider_fields = getattr(message, "provider_specific_fields", None)
            content = getattr(message, "content", None)

        if isinstance(provider_fields, dict):
            cleaned_fields = dict(provider_fields)
            if cleaned_fields.pop("thinking_blocks", None) is not None:
                stripped += 1
            if cleaned_fields.pop("reasoning_content", None) is not None:
                stripped += 1
            if cleaned_fields != provider_fields:
                if isinstance(message, dict):
                    message["provider_specific_fields"] = cleaned_fields
                else:
                    message.provider_specific_fields = cleaned_fields

        if isinstance(content, list):
            cleaned_content = [
                block
                for block in content
                if not (
                    isinstance(block, dict)
                    and block.get("type") in {"thinking", "redacted_thinking"}
                )
            ]
            if len(cleaned_content) != len(content):
                stripped += len(content) - len(cleaned_content)
                if isinstance(message, dict):
                    message["content"] = cleaned_content
                else:
                    message.content = cleaned_content

    return stripped


async def _maybe_heal_invalid_thinking_signature(
    session: Session,
    messages: list[Any],
    exc: Exception,
    *,
    already_healed: bool,
) -> bool:
    if already_healed or not _is_invalid_thinking_signature_error(exc):
        return False

    stripped = _strip_thinking_state_from_messages(messages)
    if not stripped:
        return False

    await session.send_event(
        Event(
            event_type="tool_log",
            data={
                "tool": "system",
                "log": (
                    "The inference provider rejected stale thinking signatures; retrying "
                    "without replayed thinking metadata."
                ),
            },
        )
    )
    return True


def _assistant_message_from_result(
    llm_result: LLMResult,
    *,
    tool_calls: list[ToolCall] | None = None,
) -> Message:
    """Build an assistant history message for HF Router-compatible replay."""
    kwargs: dict[str, Any] = {
        "role": "assistant",
        "content": llm_result.content,
    }
    if tool_calls is not None:
        kwargs["tool_calls"] = tool_calls
    return Message(**kwargs)


async def _call_llm_streaming(
    session: Session, messages, tools, llm_params
) -> LLMResult:
    """Call the LLM with streaming, emitting assistant_chunk events."""
    _healed_effort = False  # one-shot safety net per call
    _healed_thinking_signature = False
    t_start = time.monotonic()
    for _llm_attempt in range(_MAX_LLM_RETRIES):
        if _session_cancelled(session):
            return LLMResult(
                content=None,
                tool_calls_acc={},
                token_count=0,
                finish_reason=None,
            )
        full_content = ""
        tool_calls_acc: dict[int, dict] = {}
        token_count = 0
        finish_reason = None
        final_usage_chunk = None
        try:
            request_llm_params = with_prompt_cache_params(
                llm_params,
                session_id=router_session_id_for(session),
            )
            cached_messages, cached_tools = with_prompt_caching(
                messages, tools, request_llm_params
            )
            response = await acompletion(
                messages=cached_messages,
                tools=cached_tools,
                tool_choice="auto",
                stream=True,
                stream_options={"include_usage": True},
                timeout=600,
                **request_llm_params,
            )

            async for chunk in response:
                if session.is_cancelled:
                    tool_calls_acc.clear()
                    break

                choice = chunk.choices[0] if chunk.choices else None
                if not choice:
                    if hasattr(chunk, "usage") and chunk.usage:
                        token_count = chunk.usage.total_tokens
                        final_usage_chunk = chunk
                    continue

                delta = choice.delta
                if choice.finish_reason:
                    finish_reason = choice.finish_reason

                if delta.content:
                    full_content += delta.content
                    await session.send_event(
                        Event(
                            event_type="assistant_chunk",
                            data={"content": delta.content},
                        )
                    )

                if delta.tool_calls:
                    for tc_delta in delta.tool_calls:
                        idx = tc_delta.index
                        if idx not in tool_calls_acc:
                            tool_calls_acc[idx] = {
                                "id": "",
                                "type": "function",
                                "function": {"name": "", "arguments": ""},
                            }
                        if tc_delta.id:
                            tool_calls_acc[idx]["id"] = tc_delta.id
                        if tc_delta.function:
                            if tc_delta.function.name:
                                tool_calls_acc[idx]["function"]["name"] += (
                                    tc_delta.function.name
                                )
                            if tc_delta.function.arguments:
                                tool_calls_acc[idx]["function"]["arguments"] += (
                                    tc_delta.function.arguments
                                )

                if hasattr(chunk, "usage") and chunk.usage:
                    token_count = chunk.usage.total_tokens
                    final_usage_chunk = chunk

            usage = await telemetry.record_llm_call(
                session,
                model=llm_params.get("model", session.config.model_name),
                response=final_usage_chunk,
                latency_ms=int((time.monotonic() - t_start) * 1000),
                finish_reason=finish_reason,
            )
            return LLMResult(
                content=full_content or None,
                tool_calls_acc=tool_calls_acc,
                token_count=token_count,
                finish_reason=finish_reason,
                usage=usage,
            )
        except ContextWindowExceededError:
            raise
        except Exception as e:
            stream_received_output = bool(full_content or tool_calls_acc)
            if full_content:
                await session.send_event(
                    Event(event_type="assistant_stream_end", data={})
                )
            if stream_received_output:
                logger.warning(
                    "Streaming LLM error after partial response; not retrying "
                    "to avoid duplicating assistant output/tool calls: %s",
                    e,
                )
                await telemetry.record_llm_call(
                    session,
                    model=llm_params.get("model", session.config.model_name),
                    response=final_usage_chunk,
                    latency_ms=int((time.monotonic() - t_start) * 1000),
                    finish_reason=finish_reason or "error",
                )
                raise
            if _is_context_overflow_error(e):
                raise ContextWindowExceededError(str(e)) from e
            if not _healed_effort and _is_effort_config_error(e):
                _healed_effort = True
                llm_params = await _heal_effort_and_rebuild_params(
                    session, e, llm_params
                )
                await session.send_event(
                    Event(
                        event_type="tool_log",
                        data={
                            "tool": "system",
                            "log": "Reasoning effort not supported for this model — adjusting and retrying.",
                        },
                    )
                )
                continue
            if await _maybe_heal_invalid_thinking_signature(
                session,
                messages,
                e,
                already_healed=_healed_thinking_signature,
            ):
                _healed_thinking_signature = True
                continue
            _delay = _retry_delay_for(e, _llm_attempt)
            if _llm_attempt < _MAX_LLM_RETRIES - 1 and _delay is not None:
                logger.warning(
                    "Transient LLM error (attempt %d/%d): %s — retrying in %ds",
                    _llm_attempt + 1,
                    _MAX_LLM_RETRIES,
                    e,
                    _delay,
                )
                await session.send_event(
                    Event(
                        event_type="tool_log",
                        data={
                            "tool": "system",
                            "log": f"LLM connection error, retrying in {_delay}s...",
                        },
                    )
                )
                if await _sleep_for_retry_or_cancel(session, _delay):
                    return LLMResult(
                        content=None,
                        tool_calls_acc={},
                        token_count=0,
                        finish_reason=None,
                    )
                continue
            raise


async def _call_llm_non_streaming(
    session: Session, messages, tools, llm_params
) -> LLMResult:
    """Call the LLM without streaming, emit assistant_message at the end."""
    response = None
    _healed_effort = False
    _healed_thinking_signature = False
    t_start = time.monotonic()
    for _llm_attempt in range(_MAX_LLM_RETRIES):
        if _session_cancelled(session):
            return LLMResult(
                content=None,
                tool_calls_acc={},
                token_count=0,
                finish_reason=None,
            )
        try:
            request_llm_params = with_prompt_cache_params(
                llm_params,
                session_id=router_session_id_for(session),
            )
            cached_messages, cached_tools = with_prompt_caching(
                messages, tools, request_llm_params
            )
            response = await acompletion(
                messages=cached_messages,
                tools=cached_tools,
                tool_choice="auto",
                stream=False,
                timeout=600,
                **request_llm_params,
            )
            break
        except ContextWindowExceededError:
            raise
        except Exception as e:
            if _is_context_overflow_error(e):
                raise ContextWindowExceededError(str(e)) from e
            if not _healed_effort and _is_effort_config_error(e):
                _healed_effort = True
                llm_params = await _heal_effort_and_rebuild_params(
                    session, e, llm_params
                )
                await session.send_event(
                    Event(
                        event_type="tool_log",
                        data={
                            "tool": "system",
                            "log": "Reasoning effort not supported for this model — adjusting and retrying.",
                        },
                    )
                )
                continue
            if await _maybe_heal_invalid_thinking_signature(
                session,
                messages,
                e,
                already_healed=_healed_thinking_signature,
            ):
                _healed_thinking_signature = True
                continue
            _delay = _retry_delay_for(e, _llm_attempt)
            if _llm_attempt < _MAX_LLM_RETRIES - 1 and _delay is not None:
                logger.warning(
                    "Transient LLM error (attempt %d/%d): %s — retrying in %ds",
                    _llm_attempt + 1,
                    _MAX_LLM_RETRIES,
                    e,
                    _delay,
                )
                await session.send_event(
                    Event(
                        event_type="tool_log",
                        data={
                            "tool": "system",
                            "log": f"LLM connection error, retrying in {_delay}s...",
                        },
                    )
                )
                if await _sleep_for_retry_or_cancel(session, _delay):
                    return LLMResult(
                        content=None,
                        tool_calls_acc={},
                        token_count=0,
                        finish_reason=None,
                    )
                continue
            raise

    choice = response.choices[0]
    message = choice.message
    content = message.content or None
    finish_reason = choice.finish_reason
    token_count = response.usage.total_tokens if response.usage else 0

    # Build tool_calls_acc in the same format as streaming
    tool_calls_acc: dict[int, dict] = {}
    if message.tool_calls:
        for idx, tc in enumerate(message.tool_calls):
            tool_calls_acc[idx] = {
                "id": tc.id,
                "type": "function",
                "function": {
                    "name": tc.function.name,
                    "arguments": tc.function.arguments,
                },
            }

    # Emit the full message as a single event
    if content:
        await session.send_event(
            Event(event_type="assistant_message", data={"content": content})
        )

    usage = await telemetry.record_llm_call(
        session,
        model=llm_params.get("model", session.config.model_name),
        response=response,
        latency_ms=int((time.monotonic() - t_start) * 1000),
        finish_reason=finish_reason,
    )

    return LLMResult(
        content=content,
        tool_calls_acc=tool_calls_acc,
        token_count=token_count,
        finish_reason=finish_reason,
        usage=usage,
    )
