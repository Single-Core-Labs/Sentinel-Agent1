"""
Interactive CLI chat with the agent

Supports two modes:
  Interactive:  python -m agent.main
  Headless:     python -m agent.main "find me bird datasets"
"""

import argparse
import asyncio
import datetime
import json
import logging
import os
import re
import signal
import subprocess
import sys
import tempfile
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Optional

import litellm
from prompt_toolkit import PromptSession

from agent.config import load_config
from agent.core.agent_loop import submission_loop
from agent.core import model_switcher
from agent.core.model_ids import strip_sentinel_ai_model_prefix
from agent.core.session import OpType
from agent.core.tools import ToolRouter
from agent.messaging.gateway import NotificationGateway
from agent.utils.terminal_display import (
    format_help_text,
    get_console,
    print_approval_header,
    print_approval_item,
    print_banner,
    print_compacted,
    print_error,
    print_help,
    print_init_done,
    print_interrupted,
    print_markdown,
    print_plan,
    print_tool_call,
    print_tool_log,
    print_tool_output,
    print_turn_complete,
    print_yolo_approve,
)

litellm.drop_params = True
# Suppress the "Give Feedback / Get Help" banner LiteLLM prints to stderr
# on every error — users don't need it, and our friendly errors cover the case.
litellm.suppress_debug_info = True

CLI_CONFIG_PATH = Path(__file__).parent.parent / "configs" / "cli_agent_config.json"
logger = logging.getLogger(__name__)


def _apply_tool_runtime_override(config: Any, *, sandbox_tools: bool) -> str:
    if sandbox_tools:
        config.tool_runtime = "sandbox"
    return getattr(config, "tool_runtime", "local")


def _is_local_tool_runtime(config: Any) -> bool:
    return getattr(config, "tool_runtime", "local") == "local"


def _tool_runtime_label(local_mode: bool) -> str:
    return "local filesystem" if local_mode else "HF sandbox"


def _normalize_config_model(config: Any) -> None:
    normalized = strip_sentinel_ai_model_prefix(getattr(config, "model_name", None))
    if normalized:
        config.model_name = normalized


def _validate_cli_model_override(model: str) -> str:
    if not model_switcher.is_valid_model_id(model):
        raise ValueError(
            "Invalid model id. Use an HF Router id like "
            "'zai-org/GLM-5.2:novita' or a supported local prefix."
        )
    return model.removeprefix("sentinel-ai/")


async def _wait_for_initial_sandbox_preload(session_holder: list | None) -> None:
    session = session_holder[0] if session_holder else None
    task = getattr(session, "sandbox_preload_task", None)
    if not task:
        return
    try:
        await asyncio.shield(task)
    except asyncio.CancelledError:
        raise
    except Exception:
        # The sandbox tool will surface the stored preload error on first use.
        return


def _configure_runtime_logging() -> None:
    """Keep third-party warning spam from punching through the interactive UI."""
    import logging

    logging.getLogger("LiteLLM").setLevel(logging.ERROR)
    logging.getLogger("litellm").setLevel(logging.ERROR)


def resolve_token() -> str | None:
    """Resolve token from environment variables."""
    import os

    return os.environ.get("ADMIN_TOKEN") or None


async def _get_user_identity(token: str | None) -> tuple[str | None, str]:
    return None, "unknown"


async def _model_picker(
    config: Any,
    session_holder: list,
    prompt_session: PromptSession,
) -> None:
    """Show model provider list and let user pick one interactively."""
    from prompt_toolkit.formatted_text import HTML
    from agent.core.model_switcher import SUGGESTED_MODELS, probe_and_switch_model

    console = get_console()
    console.print(
        "\n[bold rgb(80,160,255)]Choose a model provider:[/bold rgb(80,160,255)]\n"
    )
    for i, m in enumerate(SUGGESTED_MODELS, 1):
        console.print(f"  [bold]{i}.[/bold] {m['id']}  [dim]({m['label']})[/dim]")
    console.print("  [bold]0.[/bold] Skip — keep default")
    console.print()

    try:
        choice = await prompt_session.prompt_async(
            HTML("<b>Enter number or paste model ID (<i>Enter</i> to skip): </b>")
        )
    except (EOFError, KeyboardInterrupt):
        return

    choice = choice.strip()
    if not choice:
        return

    # Check if it's a number
    try:
        num = int(choice)
        if num == 0:
            return
        if 1 <= num <= len(SUGGESTED_MODELS):
            model_id = SUGGESTED_MODELS[num - 1]["id"]
        else:
            console.print("[yellow]Invalid choice. Keeping default.[/yellow]")
            return
    except ValueError:
        model_id = choice

    if model_id:
        session = session_holder[0] if session_holder else None
        await probe_and_switch_model(
            model_id,
            config,
            session,
            console,
            resolve_token(),
        )


async def _prompt_and_save_token(prompt_session: PromptSession) -> str | None:
    """Prompt user for a token, validate it. Returns None if skipped."""
    from prompt_toolkit.formatted_text import HTML

    print("\nOptionally provide a Sentinel AI token for additional features.")
    print("Get one at: https://sentinel-ai.co/settings/tokens\n")

    try:
        token = await prompt_session.prompt_async(
            HTML("<b>Paste your token (or leave empty to skip): </b>")
        )
    except (EOFError, KeyboardInterrupt):
        return None

    token = token.strip()
    if not token:
        return None

    # Validate token against the API
    try:
        import httpx

        async with httpx.AsyncClient(timeout=10.0) as client:
            resp = await client.get(
                "https://sentinel-ai/oauth/userinfo",
                headers={"Authorization": f"Bearer {token}"},
            )
            if resp.status_code == 200:
                user_info = resp.json()
                username = user_info.get("preferred_username", "unknown")
                print(f"Token valid (user: {username})")
            else:
                print("Invalid token. Skipping.")
                return None
    except Exception:
        print("Invalid token. Skipping.")
        return None

    return token


@dataclass
class Operation:
    """Operation to be executed by the agent"""

    op_type: OpType
    data: Optional[dict[str, Any]] = None


@dataclass
class Submission:
    """Submission to the agent loop"""

    id: str
    operation: Operation


def _create_rich_console():
    """Get the shared rich Console."""
    return get_console()


def _clear_terminal() -> None:
    command = ["cmd", "/c", "cls"] if os.name == "nt" else ["clear"]
    try:
        subprocess.run(command, check=False)
    except OSError:
        pass


class _ThinkingShimmer:
    """Animated shiny/shimmer thinking indicator — a bright gradient sweeps across the text."""

    _BASE = (90, 90, 110)  # dim base color
    _HIGHLIGHT = (255, 200, 80)  # bright shimmer highlight (warm gold)
    _WIDTH = 5  # shimmer width in characters
    _FPS = 24

    def __init__(self, console):
        self._console = console
        self._task = None
        self._running = False

    def start(self):
        if self._running:
            return
        self._running = True
        self._task = asyncio.ensure_future(self._animate())

    def stop(self):
        if not self._running:
            return  # no-op when never started (e.g. headless mode)
        self._running = False
        if self._task:
            self._task.cancel()
            self._task = None
        # Clear the shimmer line
        self._console.file.write("\r\033[K")
        self._console.file.flush()

    def _render_frame(self, text: str, offset: float) -> str:
        """Render one frame: a bright spot sweeps left-to-right across `text`."""
        out = []
        n = len(text)
        for i, ch in enumerate(text):
            # Distance from the shimmer center (wraps around)
            dist = abs(i - offset)
            wrap_dist = abs(i - offset + n + self._WIDTH)
            dist = min(dist, wrap_dist, abs(i - offset - n - self._WIDTH))
            # Blend factor: 1.0 at center, 0.0 beyond _WIDTH
            t = max(0.0, 1.0 - dist / self._WIDTH)
            t = t * t * (3 - 2 * t)  # smoothstep
            r = int(self._BASE[0] + (self._HIGHLIGHT[0] - self._BASE[0]) * t)
            g = int(self._BASE[1] + (self._HIGHLIGHT[1] - self._BASE[1]) * t)
            b = int(self._BASE[2] + (self._HIGHLIGHT[2] - self._BASE[2]) * t)
            out.append(f"\033[38;2;{r};{g};{b}m{ch}")
        out.append("\033[0m")
        return "".join(out)

    async def _animate(self):
        text = "Thinking..."
        n = len(text)
        speed = 0.45  # characters per frame
        pos = 0.0
        try:
            while self._running:
                frame = self._render_frame(text, pos)
                self._console.file.write(f"\r  {frame}")
                self._console.file.flush()
                pos = (pos + speed) % (n + self._WIDTH)
                await asyncio.sleep(1.0 / self._FPS)
        except asyncio.CancelledError:
            pass


class _StreamBuffer:
    """Accumulates streamed tokens, renders markdown block-by-block as complete
    blocks appear. A "block" is everything up to a paragraph break (\\n\\n).
    Unclosed code fences (odd count of ```) hold back flushing until closed so
    a code block is always rendered as one unit."""

    def __init__(self, console):
        self._console = console
        self._buffer = ""

    def add_chunk(self, text: str):
        self._buffer += text

    def _pop_block(self) -> str | None:
        """Extract the next complete block, or return None if nothing complete."""
        if self._buffer.count("```") % 2 == 1:
            return None  # inside an open code fence — wait for close
        idx = self._buffer.find("\n\n")
        if idx == -1:
            return None
        block = self._buffer[:idx]
        self._buffer = self._buffer[idx + 2 :]
        return block

    async def flush_ready(
        self,
        cancel_event: "asyncio.Event | None" = None,
        instant: bool = False,
    ):
        """Render any complete blocks that have accumulated; leave the tail."""
        while True:
            if cancel_event is not None and cancel_event.is_set():
                return
            block = self._pop_block()
            if block is None:
                return
            if block.strip():
                await print_markdown(block, cancel_event=cancel_event, instant=instant)

    async def finish(
        self,
        cancel_event: "asyncio.Event | None" = None,
        instant: bool = False,
    ):
        """Flush complete blocks, then render whatever incomplete tail remains."""
        await self.flush_ready(cancel_event=cancel_event, instant=instant)
        if self._buffer.strip():
            await print_markdown(
                self._buffer, cancel_event=cancel_event, instant=instant
            )
        self._buffer = ""

    def discard(self):
        self._buffer = ""


async def event_listener(
    event_queue: asyncio.Queue,
    submission_queue: asyncio.Queue,
    turn_complete_event: asyncio.Event,
    ready_event: asyncio.Event,
    prompt_session: PromptSession,
    config=None,
    session_holder=None,
) -> None:
    """Background task that listens for events and displays them"""
    submission_id = [1000]
    last_tool_name = [None]
    console = _create_rich_console()
    shimmer = _ThinkingShimmer(console)
    stream_buf = _StreamBuffer(console)

    def _cancel_event():
        """Return the session's cancellation Event so print_markdown can abort
        its typewriter loop mid-stream when Ctrl+C fires."""
        s = session_holder[0] if session_holder else None
        return s._cancelled if s is not None else None

    while True:
        try:
            event = await event_queue.get()

            if event.event_type == "ready":
                tool_count = event.data.get("tool_count", 0) if event.data else 0
                print_init_done(tool_count=tool_count)
                ready_event.set()
            elif event.event_type == "assistant_message":
                shimmer.stop()
                content = event.data.get("content", "") if event.data else ""
                if content:
                    await print_markdown(content, cancel_event=_cancel_event())
            elif event.event_type == "assistant_chunk":
                content = event.data.get("content", "") if event.data else ""
                if content:
                    stream_buf.add_chunk(content)
                    # Flush any complete markdown blocks progressively so the
                    # user sees paragraphs appear as they're produced, not just
                    # at the end of the whole response.
                    shimmer.stop()
                    await stream_buf.flush_ready(cancel_event=_cancel_event())
            elif event.event_type == "assistant_stream_end":
                shimmer.stop()
                await stream_buf.finish(cancel_event=_cancel_event())
            elif event.event_type == "tool_call":
                shimmer.stop()
                stream_buf.discard()
                tool_name = event.data.get("tool", "") if event.data else ""
                arguments = event.data.get("arguments", {}) if event.data else {}
                if tool_name:
                    last_tool_name[0] = tool_name
                    # Skip printing research tool_call — the tool_log handler shows it
                    if tool_name != "research":
                        args_str = json.dumps(arguments)[:80]
                        print_tool_call(tool_name, args_str)
            elif event.event_type == "tool_output":
                output = event.data.get("output", "") if event.data else ""
                success = event.data.get("success", False) if event.data else False
                # Only show output for plan_tool — everything else is noise
                if last_tool_name[0] == "plan_tool" and output:
                    print_tool_output(output, success, truncate=False)
                shimmer.start()
            elif event.event_type == "turn_complete":
                shimmer.stop()
                stream_buf.discard()
                print_turn_complete()
                print_plan()
                session = session_holder[0] if session_holder else None
                if session is not None:
                    await session.send_deferred_turn_complete_notification(event)
                turn_complete_event.set()
            elif event.event_type == "interrupted":
                shimmer.stop()
                stream_buf.discard()
                print_interrupted()
                turn_complete_event.set()
            elif event.event_type == "undo_complete":
                console.print("[dim]Undone.[/dim]")
                turn_complete_event.set()
            elif event.event_type == "redo_complete":
                console.print("[dim]Redo complete.[/dim]")
                turn_complete_event.set()
            elif event.event_type == "new_complete":
                data = event.data or {}
                if data.get("clear_screen"):
                    _clear_terminal()
                saved_path = data.get("saved_path")
                if saved_path:
                    console.print(
                        f"[dim]Started new chat. Prior chat saved to {saved_path}.[/dim]"
                    )
                else:
                    console.print("[dim]Started new chat.[/dim]")
                turn_complete_event.set()
            elif event.event_type == "resume_complete":
                data = event.data or {}
                path = data.get("path", "?")
                count = data.get("restored_count", 0)
                dropped = int(data.get("dropped_count", 0) or 0)
                model = data.get("model_name", "?")
                invalid_model = data.get("invalid_saved_model")
                forked = bool(data.get("forked", False))
                redacted = bool(data.get("had_redacted_content", False))
                verb = "Forked from" if forked else "Resumed"
                console.print(
                    f"[green]{verb}[/green] {path} "
                    f"([cyan]{count}[/cyan] messages, "
                    f"model [cyan]{model}[/cyan])."
                )
                if dropped:
                    console.print(
                        f"[yellow]Warning:[/yellow] dropped {dropped} "
                        "malformed message(s) while restoring — surrounding "
                        "tool-call alignment may be off."
                    )
                if invalid_model:
                    console.print(
                        f"[yellow]Warning:[/yellow] saved model id "
                        f"[cyan]{invalid_model}[/cyan] failed validation; "
                        f"kept current model [cyan]{model}[/cyan]."
                    )
                if forked:
                    console.print(
                        "[dim]Saved log belongs to a different user — kept "
                        "current session id; future saves go to a fresh file.[/dim]"
                    )
                if redacted:
                    console.print(
                        "[yellow]Note:[/yellow] tokens/secrets in restored "
                        "messages were scrubbed at save time. Your live tokens "
                        "are used for this session; [REDACTED_*] markers in "
                        "past messages are not re-injected."
                    )
                turn_complete_event.set()
            elif event.event_type == "plan_generated":
                shimmer.stop()
                model = event.data.get("model", "?") if event.data else "?"
                console.print(
                    f"[bold rgb(80,160,255)]Plan phase[/bold rgb(80,160,255)] "
                    f"[dim](model: {model})[/dim]"
                )
            elif event.event_type == "step_completed":
                step_id = event.data.get("step_id", "?") if event.data else "?"
                status = event.data.get("status", "?") if event.data else "?"
                console.print(f"  [dim]Step {step_id}: {status}[/dim]")
            elif event.event_type == "observation":
                phase = event.data.get("phase", "?") if event.data else "?"
                tool_count = event.data.get("tool_count", 0) if event.data else 0
                success_count = event.data.get("success_count", 0) if event.data else 0
                if tool_count > 0:
                    summary = f"{success_count}/{tool_count} tools succeeded"
                    console.print(f"  [dim]Observe ({phase}): {summary}[/dim]")
            elif event.event_type == "tool_log":
                tool = event.data.get("tool", "") if event.data else ""
                log = event.data.get("log", "") if event.data else ""
                if log:
                    agent_id = event.data.get("agent_id", "") if event.data else ""
                    label = event.data.get("label", "") if event.data else ""
                    print_tool_log(tool, log, agent_id=agent_id, label=label)
            elif event.event_type == "tool_state_change":
                pass  # visual noise — approval flow handles this
            elif event.event_type == "error":
                shimmer.stop()
                stream_buf.discard()
                error = (
                    event.data.get("error", "Unknown error")
                    if event.data
                    else "Unknown error"
                )
                print_error(error)
                turn_complete_event.set()
            elif event.event_type == "shutdown":
                shimmer.stop()
                stream_buf.discard()
                break
            elif event.event_type == "processing":
                shimmer.start()
            elif event.event_type == "compacted":
                old_tokens = event.data.get("old_tokens", 0) if event.data else 0
                new_tokens = event.data.get("new_tokens", 0) if event.data else 0
                print_compacted(old_tokens, new_tokens)
            elif event.event_type == "approval_required":
                # Handle batch approval format
                tools_data = event.data.get("tools", []) if event.data else []
                count = event.data.get("count", 0) if event.data else 0

                if config and config.yolo_mode:
                    approvals = [
                        {
                            "tool_call_id": t.get("tool_call_id", ""),
                            "approved": True,
                            "feedback": None,
                        }
                        for t in tools_data
                    ]
                    print_yolo_approve(count)
                    submission_id[0] += 1
                    approval_submission = Submission(
                        id=f"approval_{submission_id[0]}",
                        operation=Operation(
                            op_type=OpType.EXEC_APPROVAL,
                            data={"approvals": approvals},
                        ),
                    )
                    await submission_queue.put(approval_submission)
                    continue

                print_approval_header(count)
                approvals = []

                # Ask for approval for each tool
                for i, tool_info in enumerate(tools_data, 1):
                    tool_name = tool_info.get("tool", "")
                    arguments = tool_info.get("arguments", {})
                    tool_call_id = tool_info.get("tool_call_id", "")

                    # Handle case where arguments might be a JSON string
                    if isinstance(arguments, str):
                        try:
                            arguments = json.loads(arguments)
                        except json.JSONDecodeError:
                            print(f"Warning: Failed to parse arguments for {tool_name}")
                            arguments = {}

                    operation = arguments.get("operation", "")

                    print_approval_item(i, count, tool_name, operation)

                    # Get user decision for this item. Ctrl+C / EOF here is
                    # treated as "reject remaining" (matches Codex's modal
                    # priority and Forgecode's approval-cancel path). Without
                    # this, KeyboardInterrupt kills the event listener and
                    # the main loop deadlocks waiting for turn_complete.
                    try:
                        response = await prompt_session.prompt_async(
                            f"Approve item {i}? (y=yes, yolo=approve all, n=no, or provide feedback): "
                        )
                    except (KeyboardInterrupt, EOFError):
                        get_console().print(
                            "[dim]Approval cancelled — rejecting remaining items[/dim]"
                        )
                        approvals.append(
                            {
                                "tool_call_id": tool_call_id,
                                "approved": False,
                                "feedback": "User cancelled approval",
                            }
                        )
                        for remaining in tools_data[i:]:
                            approvals.append(
                                {
                                    "tool_call_id": remaining.get("tool_call_id", ""),
                                    "approved": False,
                                    "feedback": None,
                                }
                            )
                        break

                    response = response.strip().lower()

                    # Handle yolo mode activation
                    if response == "yolo":
                        config.yolo_mode = True
                        print(
                            "YOLO MODE ACTIVATED - Auto-approving all future tool calls"
                        )
                        # Auto-approve this item and all remaining
                        approvals.append(
                            {
                                "tool_call_id": tool_call_id,
                                "approved": True,
                                "feedback": None,
                            }
                        )
                        for remaining in tools_data[i:]:
                            approvals.append(
                                {
                                    "tool_call_id": remaining.get("tool_call_id", ""),
                                    "approved": True,
                                    "feedback": None,
                                }
                            )
                        break

                    approved = response in ["y", "yes"]
                    feedback = None if approved or response in ["n", "no"] else response

                    approvals.append(
                        {
                            "tool_call_id": tool_call_id,
                            "approved": approved,
                            "feedback": feedback,
                        }
                    )

                # Submit batch approval
                submission_id[0] += 1
                approval_submission = Submission(
                    id=f"approval_{submission_id[0]}",
                    operation=Operation(
                        op_type=OpType.EXEC_APPROVAL,
                        data={"approvals": approvals},
                    ),
                )
                await submission_queue.put(approval_submission)
                console.print()  # spacing after approval
            # Silently ignore other events

        except asyncio.CancelledError:
            break
        except Exception as e:
            print(f"Event listener error: {e}")


async def get_user_input(prompt_session: PromptSession) -> str:
    """Get user input asynchronously"""
    from prompt_toolkit.formatted_text import HTML

    return await prompt_session.prompt_async(HTML("\n<b><cyan>></cyan></b> "))


# ── Runtime UI state ─────────────────────────────────────────────────────


@dataclass
class _UIPreferences:
    thinking_visible: bool = True
    details_visible: bool = True


_ui = _UIPreferences()


# ── Custom commands ──────────────────────────────────────────────────────

CUSTOM_COMMANDS_DIRS = [
    Path(".opencode") / "commands",
    Path.home() / ".config" / "opencode" / "commands",
]


def _parse_custom_command(path: Path) -> dict[str, Any] | None:
    """Parse a .md command file with YAML frontmatter.

    Returns ``{name, description, agent, model, subtask, template}`` or None.
    """
    try:
        text = path.read_text(encoding="utf-8")
    except Exception:
        return None

    name = path.stem
    desc = ""
    agent = ""
    model = ""
    subtask = False
    template = text

    # YAML frontmatter between --- markers
    m = re.match(r"^---\s*\n(.*?)\n---\s*\n(.*)", text, re.DOTALL)
    if m:
        try:
            import yaml

            meta = yaml.safe_load(m.group(1)) or {}
            desc = meta.get("description", "")
            agent = meta.get("agent", "")
            model = meta.get("model", "")
            subtask = bool(meta.get("subtask", False))
            template = m.group(2).strip()
        except Exception:
            pass

    return {
        "name": name,
        "description": desc,
        "agent": agent,
        "model": model,
        "subtask": subtask,
        "template": template,
    }


def _list_custom_commands() -> dict[str, dict[str, Any]]:
    """Scan all command dirs and return {name: spec}."""
    commands: dict[str, dict[str, Any]] = {}
    for d in CUSTOM_COMMANDS_DIRS:
        resolved = Path(d)
        if not resolved.is_dir():
            continue
        for f in sorted(resolved.glob("*.md")):
            spec = _parse_custom_command(f)
            if spec:
                commands[spec["name"]] = spec
    return commands


async def _expand_custom_command(
    name: str,
    args: str,
    prompt_session: PromptSession | None = None,
) -> str:
    """Expand a custom command template into a prompt string.

    Substitutions:
      - $ARGUMENTS, $1, $2, ... → positional args
      - !command → shell output
      - @file   → file content
    """
    commands = _list_custom_commands()
    spec = commands.get(name)
    if not spec:
        return ""
    template = spec["template"]
    arg_list = args.split() if args else []
    joined = args

    # Positional args
    template = template.replace("$ARGUMENTS", joined)
    for i, a in enumerate(arg_list, 1):
        template = template.replace(f"${i}", a)

    # Shell injection !command
    def _replace_shell(m: re.Match) -> str:
        cmd = m.group(1).strip()
        try:
            result = subprocess.run(
                cmd, shell=True, capture_output=True, text=True, timeout=30
            )
            return result.stdout or result.stderr
        except Exception as e:
            return f"<error: {e}>"

    template = re.sub(r"!`([^`]+)`", _replace_shell, template)

    # File references @file
    def _replace_file(m: re.Match) -> str:
        ref = m.group(1).strip()
        fpath = Path(ref)
        if fpath.is_file():
            try:
                return fpath.read_text(encoding="utf-8")
            except Exception:
                return f"<error: could not read {ref}>"
        return f"<file not found: {ref}>"

    template = re.sub(r"@(\S+)", _replace_file, template)

    return template


# ── Slash command helpers ────────────────────────────────────────────────


async def _resume_picker(
    arg: str,
    prompt_session: PromptSession | None,
) -> Path | None:
    """Resolve a session log path via ``arg`` or interactive selection.

    Returns ``None`` if the user cancels, no logs exist, or the argument
    matches nothing — already prints the explanation in those cases.
    """
    from agent.core.session_resume import (
        format_session_log_entry,
        list_session_logs,
        resolve_session_log_arg,
    )
    from agent.core.session import DEFAULT_SESSION_LOG_DIR

    console = get_console()
    directory = DEFAULT_SESSION_LOG_DIR
    entries = list_session_logs(directory)
    if not entries:
        console.print(f"[yellow]No session logs found in ./{directory}.[/yellow]")
        return None

    if arg:
        selected = resolve_session_log_arg(arg, entries, directory)
        if selected is None:
            console.print(f"[bold red]No matching session log:[/bold red] {arg}")
        return selected

    console.print()
    console.print("[bold]Saved sessions[/bold]")
    for index, entry in enumerate(entries, start=1):
        console.print(format_session_log_entry(index, entry))
    console.print()

    if prompt_session is None:
        console.print("[yellow]Cannot prompt for a selection here.[/yellow]")
        return None

    try:
        choice = await prompt_session.prompt_async(
            "Select session number (blank to cancel): "
        )
    except (EOFError, KeyboardInterrupt):
        console.print("[dim]Resume cancelled.[/dim]")
        return None
    choice = choice.strip()
    if not choice:
        console.print("[dim]Resume cancelled.[/dim]")
        return None
    selected = resolve_session_log_arg(choice, entries, directory)
    if selected is None:
        console.print(f"[bold red]Invalid selection:[/bold red] {choice}")
    return selected


async def _handle_slash_command(
    cmd: str,
    config,
    session_holder: list,
    submission_queue: asyncio.Queue,
    submission_id: list[int],
    prompt_session: PromptSession | None = None,
) -> Submission | None:
    """
    Handle a slash command. Returns a Submission to enqueue, or None if
    the command was handled locally (caller should set turn_complete_event).

    Async because ``/model`` fires a probe ping to validate the model+effort
    combo before committing the switch.
    """
    parts = cmd.strip().split(None, 1)
    command = parts[0].lower()
    arg = parts[1].strip() if len(parts) > 1 else ""
    console = get_console()

    # ── Check custom commands first ─────────────────────────────────────
    custom = _list_custom_commands()
    stripped = command.lstrip("/")
    if stripped in custom:
        expanded = await _expand_custom_command(stripped, arg, prompt_session)
        if expanded:
            submission_id[0] += 1
            return Submission(
                id=f"sub_{submission_id[0]}",
                operation=Operation(
                    op_type=OpType.USER_INPUT,
                    data={"text": expanded},
                ),
            )
        console.print(f"[yellow]Custom command /{stripped} expanded to empty.[/yellow]")
        return None

    # ── Built-in commands ───────────────────────────────────────────────

    if command == "/help":
        custom_descs = "\n".join(
            f"  /{name}  {spec['description'] or '(custom command)'}"
            for name, spec in sorted(custom.items())
        )
        if custom_descs:
            full_help = format_help_text()
            console.print()
            console.print(full_help)
            console.print("[bold]Custom commands[/bold]")
            console.print(custom_descs)
            console.print()
        else:
            print_help()
        return None

    if command == "/undo":
        submission_id[0] += 1
        return Submission(
            id=f"sub_{submission_id[0]}",
            operation=Operation(op_type=OpType.UNDO),
        )

    if command == "/redo":
        submission_id[0] += 1
        return Submission(
            id=f"sub_{submission_id[0]}",
            operation=Operation(op_type=OpType.REDO),
        )

    if command == "/compact":
        submission_id[0] += 1
        return Submission(
            id=f"sub_{submission_id[0]}",
            operation=Operation(op_type=OpType.COMPACT),
        )

    if command in {"/new", "/clear"}:
        session = session_holder[0] if session_holder else None
        if session is None:
            get_console().print("[bold red]No active session to reset.[/bold red]")
            return None
        submission_id[0] += 1
        return Submission(
            id=f"sub_{submission_id[0]}",
            operation=Operation(
                op_type=OpType.NEW,
                data={"clear_screen": command == "/clear"},
            ),
        )

    if command == "/resume":
        session = session_holder[0] if session_holder else None
        if session is None:
            get_console().print(
                "[bold red]No active session to restore into.[/bold red]"
            )
            return None
        selected_path = await _resume_picker(arg, prompt_session)
        if selected_path is None:
            return None
        submission_id[0] += 1
        return Submission(
            id=f"sub_{submission_id[0]}",
            operation=Operation(
                op_type=OpType.RESUME, data={"path": str(selected_path)}
            ),
        )

    if command in {"/sessions", "/continue"}:
        session = session_holder[0] if session_holder else None
        if session is None:
            get_console().print("[bold red]No active session.[/bold red]")
            return None
        # /sessions lists all; /continue alias is a no-arg /resume
        from agent.core.session_resume import (
            format_session_log_entry,
            list_session_logs,
            resolve_session_log_arg,
        )
        from agent.core.session import DEFAULT_SESSION_LOG_DIR

        directory = DEFAULT_SESSION_LOG_DIR
        entries = list_session_logs(directory)
        if not entries:
            console.print(f"[yellow]No session logs found in ./{directory}.[/yellow]")
            return None
        if arg:
            selected = resolve_session_log_arg(arg, entries, directory)
            if selected is None:
                console.print(f"[bold red]No matching session:[/bold red] {arg}")
                return None
            submission_id[0] += 1
            return Submission(
                id=f"sub_{submission_id[0]}",
                operation=Operation(
                    op_type=OpType.RESUME, data={"path": str(selected)}
                ),
            )
        # List sessions interactively
        console.print()
        console.print("[bold]Saved sessions[/bold]")
        for index, entry in enumerate(entries, start=1):
            console.print(format_session_log_entry(index, entry))
        console.print()
        if prompt_session is None:
            return None
        try:
            choice = await prompt_session.prompt_async(
                "Select session number or path (blank to cancel): "
            )
        except (EOFError, KeyboardInterrupt):
            console.print("[dim]Cancelled.[/dim]")
            return None
        choice = choice.strip()
        if not choice:
            return None
        selected = resolve_session_log_arg(choice, entries, directory)
        if selected is None:
            console.print(f"[bold red]Invalid selection:[/bold red] {choice}")
            return None
        submission_id[0] += 1
        return Submission(
            id=f"sub_{submission_id[0]}",
            operation=Operation(op_type=OpType.RESUME, data={"path": str(selected)}),
        )

    if command in {"/model", "/models"}:
        if not arg and command == "/models":
            model_switcher.print_model_listing(config, console)
            return None
        if not arg:
            model_switcher.print_model_listing(config, console)
            return None
        if not model_switcher.is_valid_model_id(arg):
            model_switcher.print_invalid_id(arg, console)
            return None
        normalized = arg.removeprefix("sentinel-ai/")
        session = session_holder[0] if session_holder else None
        await model_switcher.probe_and_switch_model(
            normalized,
            config,
            session,
            console,
            resolve_token(),
        )
        return None

    if command == "/yolo":
        config.yolo_mode = not config.yolo_mode
        state = "ON" if config.yolo_mode else "OFF"
        print(f"YOLO mode: {state}")
        return None

    if command == "/effort":
        valid = {"minimal", "low", "medium", "high", "xhigh", "max", "off"}
        session = session_holder[0] if session_holder else None
        if not arg:
            current = config.reasoning_effort or "off"
            console.print(f"[bold]Reasoning effort preference:[/bold] {current}")
            if session and session.model_effective_effort:
                console.print("[dim]Probed per model:[/dim]")
                for m, eff in session.model_effective_effort.items():
                    console.print(f"  [dim]{m}: {eff or 'off'}[/dim]")
            console.print(
                "[dim]Set with '/effort minimal|low|medium|high|xhigh|max|off'. "
                "HF Router accepts low|medium|high generically; higher preferences "
                "are probed and the cascade falls back to whatever the selected "
                "provider accepts.[/dim]"
            )
            return None
        level = arg.lower()
        if level not in valid:
            console.print(f"[bold red]Invalid level:[/bold red] {arg}")
            console.print(f"[dim]Expected one of: {', '.join(sorted(valid))}[/dim]")
            return None
        config.reasoning_effort = None if level == "off" else level
        if session is not None:
            session.model_effective_effort.clear()
        console.print(f"[green]Reasoning effort: {level}[/green]")
        if session is not None:
            console.print(
                "[dim]run /model <current> to re-probe, or send a message — "
                "the agent adjusts automatically if the new level isn't supported.[/dim]"
            )
        return None

    if command == "/status":
        session = session_holder[0] if session_holder else None
        print(f"Model: {config.model_name}")
        print(f"Reasoning effort: {config.reasoning_effort or 'off'}")
        print(f"Tool runtime: {_tool_runtime_label(_is_local_tool_runtime(config))}")
        if session:
            print(f"Turns: {session.turn_count}")
            print(f"Context items: {len(session.context_manager.items)}")
        return None

    if command == "/thinking":
        _ui.thinking_visible = not _ui.thinking_visible
        state = "visible" if _ui.thinking_visible else "hidden"
        console.print(f"[dim]Thinking blocks: {state}[/dim]")
        return None

    if command == "/details":
        _ui.details_visible = not _ui.details_visible
        state = "visible" if _ui.details_visible else "hidden"
        console.print(f"[dim]Tool execution details: {state}[/dim]")
        return None

    if command == "/editor":
        if prompt_session is None:
            console.print("[yellow]No terminal session available for editor.[/yellow]")
            return None
        editor = os.environ.get("EDITOR", "notepad" if os.name == "nt" else "nano")
        tmp = tempfile.NamedTemporaryFile(
            mode="w+",
            suffix=".md",
            delete=False,
            encoding="utf-8",
        )
        tmp.write("# Compose your message\n\n")
        tmp.close()
        try:
            subprocess.run([editor, tmp.name], check=False)
            content = Path(tmp.name).read_text(encoding="utf-8").strip()
            # Strip the heading hint
            lines = content.splitlines()
            clean = "\n".join(
                line for line in lines if line != "# Compose your message"
            ).strip()
            if not clean:
                console.print("[dim]Empty message — cancelled.[/dim]")
                return None
            submission_id[0] += 1
            return Submission(
                id=f"sub_{submission_id[0]}",
                operation=Operation(
                    op_type=OpType.USER_INPUT,
                    data={"text": clean},
                ),
            )
        finally:
            Path(tmp.name).unlink(missing_ok=True)

    if command == "/export":
        session = session_holder[0] if session_holder else None
        if session is None:
            console.print("[bold red]No active session to export.[/bold red]")
            return None
        ts = datetime.datetime.now().strftime("%Y%m%d_%H%M%S")
        export_dir = Path("exports")
        export_dir.mkdir(exist_ok=True)
        path = export_dir / f"session_{ts}.md"
        lines_out = [f"# Session Export ({ts})\n"]
        for msg in session.context_manager.items:
            role = getattr(msg, "role", "unknown")
            content = getattr(msg, "content", "") or ""
            if role == "system":
                continue
            lines_out.append(f"\n## {role}\n")
            lines_out.append(str(content))
        path.write_text("\n".join(lines_out), encoding="utf-8")
        console.print(f"[green]Session exported to[/green] {path}")
        return None

    if command == "/connect":
        console.print("[bold]Configure a provider[/bold]")
        console.print(
            "Set your API keys in the environment or in a `.env` file.\n"
            "Available providers: OpenAI, Anthropic, Google, "
            "NVIDIA NIM (NVIDIA_NIM_API_KEY), and more via LiteLLM.\n"
            "For custom endpoints, set OPENAI_API_KEY and OPENAI_BASE_URL.\n"
            "[dim]Example: set OPENAI_API_KEY=sk-... in your .env[/dim]"
        )
        return None

    if command == "/init":
        console.print("[bold]Guided AGENTS.md setup[/bold]")
        agents_path = Path("AGENTS.md")
        if agents_path.exists():
            console.print(
                f"[green]AGENTS.md already exists[/green] ({agents_path.resolve()})"
            )
            console.print(
                "[dim]Edit it manually to add project-specific instructions "
                "for the agent.[/dim]"
            )
        else:
            template = (
                "# Agent Notes\n\n"
                "## Development\n\n"
                "- Run tests: <command>\n"
                "- Build: <command>\n"
                "- Lint: <command>\n"
                "- Typecheck: <command>\n"
                "\n"
                "## Conventions\n\n"
                "- <style guide, patterns, etc.>\n"
            )
            agents_path.write_text(template, encoding="utf-8")
            console.print(f"[green]Created AGENTS.md[/green] ({agents_path.resolve()})")
            console.print("[dim]Edit it with project-specific instructions.[/dim]")
        return None

    if command == "/share":
        session = session_holder[0] if session_holder else None
        if session is None:
            console.print("[bold red]No active session to share.[/bold red]")
            return None
        console.print(
            "[yellow]Session sharing is not yet available.[/yellow]\n"
            "[dim]Use /export to save the conversation to a Markdown file.[/dim]"
        )
        return None

    if command == "/unshare":
        console.print("[yellow]Session sharing is not yet available.[/yellow]")
        return None

    if command == "/themes":
        console.print(
            "[bold]Available themes[/bold]\n"
            "  default  - Default terminal colors\n"
            "[dim]Theme customization is not yet implemented. "
            "Set TERMINAL_THEME env var for future support.[/dim]"
        )
        return None

    if command == "/share-traces":
        session = session_holder[0] if session_holder else None
        await _handle_share_traces_command(arg, config, session)
        return None

    # Custom command name without / prefix
    console.print(
        f"[yellow]Unknown command: {command}.[/yellow] "
        "[dim]Type /help for available commands.[/dim]"
    )
    return None


async def _handle_share_traces_command(arg: str, config, session) -> None:
    console = get_console()
    console.print("[dim]Trace sharing is no longer available.[/dim]")


async def main(model: str | None = None, sandbox_tools: bool = False):
    """Interactive chat with the agent"""

    # Clear screen
    _clear_terminal()

    # Create prompt session for input (needed early for token prompt)
    prompt_session = PromptSession()

    config = load_config(CLI_CONFIG_PATH, include_user_defaults=True)
    _normalize_config_model(config)
    if model:
        config.model_name = _validate_cli_model_override(model)
    _apply_tool_runtime_override(config, sandbox_tools=sandbox_tools)
    local_mode = _is_local_tool_runtime(config)

    hf_token = resolve_token()

    # Resolve username for banner.
    user, _ = await _get_user_identity(hf_token)

    print_banner(
        model=config.model_name,
        user=user,
        tool_runtime=_tool_runtime_label(local_mode),
    )

    # Create queues for communication
    submission_queue = asyncio.Queue()
    event_queue = asyncio.Queue()

    # Events to signal agent state
    turn_complete_event = asyncio.Event()
    turn_complete_event.set()
    ready_event = asyncio.Event()

    notification_gateway = NotificationGateway(config.messaging)
    await notification_gateway.start()
    # Create tool router with the selected CLI tool runtime.
    tool_router = ToolRouter(
        config.mcpServers, hf_token=hf_token, local_mode=local_mode
    )

    # Session holder for interrupt/model/status access
    session_holder = [None]

    agent_task = asyncio.create_task(
        submission_loop(
            submission_queue,
            event_queue,
            config=config,
            tool_router=tool_router,
            session_holder=session_holder,
            hf_token=hf_token,
            user_id=user,
            local_mode=local_mode,
            autonomous_mode=False,
            stream=True,
            notification_gateway=notification_gateway,
            notification_destinations=config.messaging.default_auto_destinations(),
            defer_turn_complete_notification=True,
        )
    )

    # Start event listener in background
    listener_task = asyncio.create_task(
        event_listener(
            event_queue,
            submission_queue,
            turn_complete_event,
            ready_event,
            prompt_session,
            config,
            session_holder=session_holder,
        )
    )

    await ready_event.wait()
    if not local_mode:
        await _wait_for_initial_sandbox_preload(session_holder)

    await _model_picker(config, session_holder, prompt_session)

    submission_id = [0]
    # Mirrors codex-rs/tui/src/bottom_pane/mod.rs:137
    # (`QUIT_SHORTCUT_TIMEOUT = Duration::from_secs(1)`). Two Ctrl+C presses
    # within this window quit; a single press cancels the in-flight turn.
    CTRL_C_QUIT_WINDOW = 1.0
    # Hint string matches codex-rs/tui/src/bottom_pane/footer.rs:746
    # (`" again to quit"` prefixed with the key binding, rendered dim).
    CTRL_C_HINT = "[dim]ctrl + c again to quit[/dim]"
    interrupt_state = {"last": 0.0, "exit": False}

    loop = asyncio.get_running_loop()

    def _on_sigint() -> None:
        """SIGINT handler — fires while the agent is generating (terminal is
        in cooked mode between prompts). Mirrors Codex's `on_ctrl_c` in
        codex-rs/tui/src/chatwidget.rs: first press cancels active work and
        arms the quit hint; second press within the window quits."""
        now = time.monotonic()
        session = session_holder[0]

        if now - interrupt_state["last"] < CTRL_C_QUIT_WINDOW:
            interrupt_state["exit"] = True
            if session:
                session.cancel()
            # Wake the main loop out of turn_complete_event.wait()
            turn_complete_event.set()
            return

        interrupt_state["last"] = now
        if session and not session.is_cancelled:
            session.cancel()
        get_console().print(f"\n{CTRL_C_HINT}")

    def _install_sigint() -> bool:
        try:
            loop.add_signal_handler(signal.SIGINT, _on_sigint)
            return True
        except (NotImplementedError, RuntimeError):
            return False  # Windows or non-main thread

    # prompt_toolkit's prompt_async installs its own SIGINT handler and, on
    # exit, calls loop.remove_signal_handler(SIGINT) — which wipes ours too.
    # So we re-arm at the top of every loop iteration, right before the busy
    # wait. Without this, Ctrl+C during agent streaming after the first turn
    # falls through to the default handler and the terminal just echoes ^C.
    sigint_available = _install_sigint()

    try:
        while True:
            if sigint_available:
                _install_sigint()

            try:
                await turn_complete_event.wait()
            except asyncio.CancelledError:
                break
            turn_complete_event.clear()

            if interrupt_state["exit"]:
                break

            # Get user input. prompt_toolkit puts the terminal in raw mode and
            # installs its own SIGINT handling; ^C arrives as \x03 and surfaces
            # as KeyboardInterrupt here. On return, prompt_toolkit removes the
            # loop's SIGINT handler — we re-arm at the top of the next iter.
            try:
                user_input = await get_user_input(prompt_session)
            except EOFError:
                break
            except KeyboardInterrupt:
                now = time.monotonic()
                if now - interrupt_state["last"] < CTRL_C_QUIT_WINDOW:
                    break
                interrupt_state["last"] = now
                get_console().print(CTRL_C_HINT)
                turn_complete_event.set()
                continue

            # A successful read ends the double-press window — an unrelated
            # Ctrl+C during the next turn should start a fresh arming.
            interrupt_state["last"] = 0.0

            # Check for exit commands
            if user_input.strip().lower() in ["exit", "quit", "/quit", "/exit"]:
                break

            # Skip empty input
            if not user_input.strip():
                turn_complete_event.set()
                continue

            # Handle slash commands
            if user_input.strip().startswith("/"):
                sub = await _handle_slash_command(
                    user_input.strip(),
                    config,
                    session_holder,
                    submission_queue,
                    submission_id,
                    prompt_session,
                )
                if sub is None:
                    # Command handled locally, loop back for input
                    turn_complete_event.set()
                    continue
                else:
                    await submission_queue.put(sub)
                    continue

            # Submit to agent
            submission_id[0] += 1
            submission = Submission(
                id=f"sub_{submission_id[0]}",
                operation=Operation(
                    op_type=OpType.USER_INPUT, data={"text": user_input}
                ),
            )
            await submission_queue.put(submission)

    except KeyboardInterrupt:
        pass
    finally:
        if sigint_available:
            try:
                loop.remove_signal_handler(signal.SIGINT)
            except (NotImplementedError, RuntimeError):
                pass

    # Shutdown
    shutdown_submission = Submission(
        id="sub_shutdown", operation=Operation(op_type=OpType.SHUTDOWN)
    )
    await submission_queue.put(shutdown_submission)

    # Wait for agent to finish (the listener must keep draining events
    # or the agent will block on event_queue.put)
    try:
        await asyncio.wait_for(agent_task, timeout=10.0)
    except asyncio.TimeoutError:
        agent_task.cancel()
        # Agent didn't shut down cleanly — close MCP explicitly
        await tool_router.__aexit__(None, None, None)
    finally:
        await notification_gateway.close()

    # Now safe to cancel the listener (agent is done emitting events)
    listener_task.cancel()

    get_console().print("\n[dim]Bye.[/dim]\n")


async def headless_main(
    prompt: str,
    model: str | None = None,
    max_iterations: int | None = None,
    stream: bool = True,
    sandbox_tools: bool = False,
) -> None:
    """Run a single prompt headlessly and exit."""
    import logging

    logging.basicConfig(level=logging.WARNING)
    _configure_runtime_logging()

    config = load_config(CLI_CONFIG_PATH, include_user_defaults=True)
    _normalize_config_model(config)
    config.yolo_mode = True  # Auto-approve everything in headless mode

    if model:
        try:
            config.model_name = _validate_cli_model_override(model)
        except ValueError as e:
            print(f"ERROR: {e}", file=sys.stderr)
            sys.exit(1)
    _apply_tool_runtime_override(config, sandbox_tools=sandbox_tools)
    local_mode = _is_local_tool_runtime(config)

    hf_token = resolve_token()
    if hf_token:
        print("Token loaded", file=sys.stderr)

    notification_gateway = NotificationGateway(config.messaging)
    await notification_gateway.start()
    user, _ = await _get_user_identity(hf_token)

    if max_iterations is not None:
        config.max_iterations = max_iterations

    print(f"Model: {config.model_name}", file=sys.stderr)
    print(f"Tool runtime: {_tool_runtime_label(local_mode)}", file=sys.stderr)
    print(f"Max iterations: {config.max_iterations}", file=sys.stderr)
    print(f"Prompt: {prompt}", file=sys.stderr)
    print("---", file=sys.stderr)

    submission_queue: asyncio.Queue = asyncio.Queue()
    event_queue: asyncio.Queue = asyncio.Queue()

    tool_router = ToolRouter(
        config.mcpServers, hf_token=hf_token, local_mode=local_mode
    )
    session_holder: list = [None]

    agent_task = asyncio.create_task(
        submission_loop(
            submission_queue,
            event_queue,
            config=config,
            tool_router=tool_router,
            session_holder=session_holder,
            hf_token=hf_token,
            user_id=user,
            local_mode=local_mode,
            autonomous_mode=True,
            stream=stream,
            notification_gateway=notification_gateway,
            notification_destinations=config.messaging.default_auto_destinations(),
            defer_turn_complete_notification=True,
        )
    )

    # Wait for ready
    while True:
        event = await event_queue.get()
        if event.event_type == "ready":
            break

    # Submit the prompt
    submission = Submission(
        id="sub_1",
        operation=Operation(op_type=OpType.USER_INPUT, data={"text": prompt}),
    )
    await submission_queue.put(submission)

    # Process events until turn completes. Headless mode is for scripts /
    # log capture: no shimmer animation, no typewriter, no live-redrawing
    # research overlay. Output is plain, append-only text.
    console = _create_rich_console()
    stream_buf = _StreamBuffer(console)
    _hl_last_tool = [None]
    _hl_sub_id = [1]
    # Research sub-agent tool calls are buffered per agent_id and dumped as
    # a static block once each sub-agent finishes, instead of streaming via
    # the live redrawing SubAgentDisplayManager (which is TTY-only).
    _hl_research_buffers: dict[str, dict] = {}

    while True:
        event = await event_queue.get()

        if event.event_type == "assistant_chunk":
            content = event.data.get("content", "") if event.data else ""
            if content:
                stream_buf.add_chunk(content)
                await stream_buf.flush_ready(instant=True)
        elif event.event_type == "assistant_stream_end":
            await stream_buf.finish(instant=True)
        elif event.event_type == "assistant_message":
            content = event.data.get("content", "") if event.data else ""
            if content:
                await print_markdown(content, instant=True)
        elif event.event_type == "tool_call":
            stream_buf.discard()
            tool_name = event.data.get("tool", "") if event.data else ""
            arguments = event.data.get("arguments", {}) if event.data else {}
            if tool_name:
                _hl_last_tool[0] = tool_name
                if tool_name != "research":
                    args_str = json.dumps(arguments)[:80]
                    print_tool_call(tool_name, args_str)
        elif event.event_type == "tool_output":
            output = event.data.get("output", "") if event.data else ""
            success = event.data.get("success", False) if event.data else False
            if _hl_last_tool[0] == "plan_tool" and output:
                print_tool_output(output, success, truncate=False)
        elif event.event_type == "tool_log":
            tool = event.data.get("tool", "") if event.data else ""
            log = event.data.get("log", "") if event.data else ""
            if not log:
                pass
            elif tool == "research":
                # Headless mode: buffer research sub-agent activity per-agent,
                # then dump each as a static block on completion. The live
                # SubAgentDisplayManager uses terminal cursor tricks that are
                # unfit for non-TTY output, but parallel agents still need
                # distinct output so we key buffers by agent_id.
                agent_id = event.data.get("agent_id", "") if event.data else ""
                label = event.data.get("label", "") if event.data else ""
                aid = agent_id or "research"
                if log == "Starting research sub-agent...":
                    _hl_research_buffers[aid] = {
                        "label": label or "research",
                        "calls": [],
                    }
                elif log == "Research complete.":
                    buf = _hl_research_buffers.pop(aid, None)
                    if buf is not None:
                        f = get_console().file
                        f.write(f"  \033[38;2;255;200;80m▸ {buf['label']}\033[0m\n")
                        for call in buf["calls"]:
                            f.write(f"    \033[2m{call}\033[0m\n")
                        f.flush()
                elif log.startswith("tokens:") or log.startswith("tools:"):
                    pass  # stats updates — only useful for the live display
                elif aid in _hl_research_buffers:
                    _hl_research_buffers[aid]["calls"].append(log)
                else:
                    # Orphan event (Start was missed) — fall back to raw print
                    print_tool_log(tool, log, agent_id=agent_id, label=label)
            else:
                print_tool_log(tool, log)
        elif event.event_type == "approval_required":
            # Auto-approve in headless mode, except scheduled HF jobs. Those
            # are rejected because their recurring cost needs manual approval.
            tools_data = event.data.get("tools", []) if event.data else []
            approvals = [
                {
                    "tool_call_id": t.get("tool_call_id", ""),
                    "approved": True,
                    "feedback": None,
                }
                for t in tools_data
            ]
            _hl_sub_id[0] += 1
            await submission_queue.put(
                Submission(
                    id=f"hl_approval_{_hl_sub_id[0]}",
                    operation=Operation(
                        op_type=OpType.EXEC_APPROVAL,
                        data={"approvals": approvals},
                    ),
                )
            )
        elif event.event_type == "compacted":
            old_tokens = event.data.get("old_tokens", 0) if event.data else 0
            new_tokens = event.data.get("new_tokens", 0) if event.data else 0
            print_compacted(old_tokens, new_tokens)
        elif event.event_type == "error":
            stream_buf.discard()
            error = (
                event.data.get("error", "Unknown error")
                if event.data
                else "Unknown error"
            )
            print_error(error)
            break
        elif event.event_type in ("turn_complete", "interrupted"):
            stream_buf.discard()
            history_size = event.data.get("history_size", "?") if event.data else "?"
            print(
                f"\n--- Agent {event.event_type} (history_size={history_size}) ---",
                file=sys.stderr,
            )
            if event.event_type == "turn_complete":
                session = session_holder[0] if session_holder else None
                if session is not None:
                    await session.send_deferred_turn_complete_notification(event)
            break

    # Shutdown
    shutdown_submission = Submission(
        id="sub_shutdown", operation=Operation(op_type=OpType.SHUTDOWN)
    )
    await submission_queue.put(shutdown_submission)

    try:
        await asyncio.wait_for(agent_task, timeout=10.0)
    except asyncio.TimeoutError:
        agent_task.cancel()
        await tool_router.__aexit__(None, None, None)
    finally:
        await notification_gateway.close()


async def _ipc_event_writer(event_queue: asyncio.Queue):
    while True:
        try:
            event = await event_queue.get()
            # Serialize the event safely
            data = event.data if isinstance(event.data, dict) else {}
            # Need to handle potential non-serializable objects (like datetime)
            import json

            try:
                line = json.dumps({"type": event.event_type, "data": data}, default=str)
                sys.stdout.write(line + "\n")
                sys.stdout.flush()
            except Exception:
                pass  # Silently drop serialization errors in IPC mode
            if event.event_type == "shutdown":
                break
        except asyncio.CancelledError:
            break
        except Exception:
            pass


async def _ipc_input_reader(submission_queue: asyncio.Queue, config, session_holder):
    import sys
    import json

    loop = asyncio.get_running_loop()
    sub_id = [0]

    # Read from stdin continuously
    while True:
        try:
            line = await loop.run_in_executor(None, sys.stdin.readline)
            if not line:
                break
            line = line.strip()
            if not line:
                continue

            try:
                cmd = json.loads(line)
            except json.JSONDecodeError:
                continue

            op_type_str = cmd.get("op_type")
            data = cmd.get("data", {})

            op_type = None
            if op_type_str == "USER_INPUT":
                op_type = OpType.USER_INPUT
                # Handle slash commands via JSON directly if they want, but let's just forward text
            elif op_type_str == "EXEC_APPROVAL":
                op_type = OpType.EXEC_APPROVAL
            elif op_type_str == "UNDO":
                op_type = OpType.UNDO
            elif op_type_str == "REDO":
                op_type = OpType.REDO
            elif op_type_str == "COMPACT":
                op_type = OpType.COMPACT
            elif op_type_str == "NEW":
                op_type = OpType.NEW
            elif op_type_str == "RESUME":
                op_type = OpType.RESUME
            elif op_type_str == "SHUTDOWN":
                op_type = OpType.SHUTDOWN
            else:
                continue

            sub_id[0] += 1
            submission = Submission(
                id=f"ipc_sub_{sub_id[0]}",
                operation=Operation(op_type=op_type, data=data),
            )
            await submission_queue.put(submission)

            if op_type == OpType.SHUTDOWN:
                break
        except Exception:
            break


async def ipc_main(model: str | None = None, sandbox_tools: bool = False):
    """IPC JSON loop for external frontends."""
    import logging

    logging.getLogger().setLevel(logging.CRITICAL)
    _configure_runtime_logging()

    config = load_config(CLI_CONFIG_PATH, include_user_defaults=True)
    _normalize_config_model(config)
    if model:
        try:
            config.model_name = _validate_cli_model_override(model)
        except ValueError:
            pass
    _apply_tool_runtime_override(config, sandbox_tools=sandbox_tools)
    local_mode = _is_local_tool_runtime(config)

    hf_token = resolve_token()
    user, _ = await _get_user_identity(hf_token)

    notification_gateway = NotificationGateway(config.messaging)
    await notification_gateway.start()

    submission_queue: asyncio.Queue = asyncio.Queue()
    event_queue: asyncio.Queue = asyncio.Queue()

    tool_router = ToolRouter(
        config.mcpServers, hf_token=hf_token, local_mode=local_mode
    )
    session_holder: list = [None]

    agent_task = asyncio.create_task(
        submission_loop(
            submission_queue,
            event_queue,
            config=config,
            tool_router=tool_router,
            session_holder=session_holder,
            hf_token=hf_token,
            user_id=user,
            local_mode=local_mode,
            autonomous_mode=False,
            stream=True,
            notification_gateway=notification_gateway,
            notification_destinations=config.messaging.default_auto_destinations(),
            defer_turn_complete_notification=True,
        )
    )

    writer_task = asyncio.create_task(_ipc_event_writer(event_queue))
    reader_task = asyncio.create_task(
        _ipc_input_reader(submission_queue, config, session_holder)
    )

    try:
        await asyncio.gather(reader_task, agent_task)
    except asyncio.CancelledError:
        pass
    finally:
        writer_task.cancel()
        agent_task.cancel()
        await tool_router.__aexit__(None, None, None)
        await notification_gateway.close()


def _launch_ink_ui(model: str | None, sandbox_tools: bool) -> int:
    """Launch the Node/Ink terminal UI. Returns process exit code."""
    import shutil
    import subprocess

    # Find the frontend directory relative to this file (agent/main.py)
    # Structure: <root>/agent/main.py  →  <root>/frontend/
    agent_dir = os.path.dirname(os.path.abspath(__file__))
    root_dir = os.path.dirname(agent_dir)
    frontend_dir = os.path.join(root_dir, "frontend")
    entry = os.path.join(frontend_dir, "src", "index.tsx")

    if not os.path.isfile(entry):
        return -1  # Frontend not found — caller falls back to Python REPL

    # Prefer local tsx binary so we don't need a global install
    tsx_candidates = [
        os.path.join(frontend_dir, "node_modules", ".bin", "tsx.cmd"),
        os.path.join(frontend_dir, "node_modules", ".bin", "tsx"),
        shutil.which("tsx"),
    ]
    tsx = next((t for t in tsx_candidates if t and os.path.isfile(t)), None)

    if tsx is None:
        return -1  # No tsx available — fall back

    cmd = [tsx, entry]
    if model:
        cmd += ["--model", model]
    if sandbox_tools:
        cmd += ["--sandbox-tools"]

    try:
        result = subprocess.run(
            cmd,
            cwd=frontend_dir,
            env={**os.environ, "FORCE_COLOR": "1"},
        )
        return result.returncode
    except Exception:
        return -1


def cli():
    """Entry point for the platform-agent / sentinel-ai CLI command."""
    import logging as _logging
    import warnings

    _logging.getLogger("asyncio").setLevel(_logging.CRITICAL)
    _configure_runtime_logging()
    warnings.filterwarnings("ignore", category=DeprecationWarning, module="litellm")
    warnings.filterwarnings("ignore", category=SyntaxWarning, module="whoosh")

    parser = argparse.ArgumentParser(description="Sentinel AI Agent CLI")
    parser.add_argument(
        "prompt", nargs="?", default=None, help="Run headlessly with this prompt"
    )
    parser.add_argument(
        "--model", "-m", default=None, help="Model to use (default: from config)"
    )
    parser.add_argument(
        "--max-iterations",
        type=int,
        default=None,
        help="Max LLM requests per turn (default: 50, use -1 for unlimited)",
    )
    parser.add_argument(
        "--no-stream",
        action="store_true",
        help="Disable token streaming (use non-streaming LLM calls)",
    )
    parser.add_argument(
        "--sandbox-tools",
        action="store_true",
        help="Use HF Space sandbox tools instead of local filesystem tools",
    )
    parser.add_argument(
        "--json-ipc",
        action="store_true",
        help="Run in JSON IPC mode for the Ink UI (internal use)",
    )
    parser.add_argument(
        "--python-repl",
        action="store_true",
        help="Force the legacy Python prompt_toolkit REPL instead of the Ink UI",
    )
    args = parser.parse_args()

    try:
        if args.json_ipc:
            asyncio.run(ipc_main(model=args.model, sandbox_tools=args.sandbox_tools))
        elif args.prompt:
            max_iter = args.max_iterations
            if max_iter is not None and max_iter < 0:
                max_iter = 10_000
            asyncio.run(
                headless_main(
                    args.prompt,
                    model=args.model,
                    max_iterations=max_iter,
                    stream=not args.no_stream,
                    sandbox_tools=args.sandbox_tools,
                )
            )
        elif args.python_repl:
            asyncio.run(main(model=args.model, sandbox_tools=args.sandbox_tools))
        else:
            # Default interactive mode: launch the Ink UI
            code = _launch_ink_ui(args.model, args.sandbox_tools)
            if code == -1:
                # Ink UI unavailable — fall back to Python REPL
                asyncio.run(main(model=args.model, sandbox_tools=args.sandbox_tools))
            elif code != 0:
                sys.exit(code)
    except KeyboardInterrupt:
        print("\n\nGoodbye!")


if __name__ == "__main__":
    cli()
