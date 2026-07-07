"""Opt-in live sandbox communication test.

This test creates a real private PlatformOps Space sandbox, verifies that
unauthenticated requests are rejected, then exercises the authenticated agent
client end-to-end.
It is skipped unless ``SENTINEL_AI_LIVE_SANDBOX_TESTS=1`` and ``HF_TOKEN`` are set.
"""

from __future__ import annotations

import os
from pathlib import Path

import httpx
import pytest
from dotenv import load_dotenv
from huggingface_hub import HfApi

from agent.tools.sandbox_client import Sandbox


if env_file := os.environ.get("SENTINEL_AI_LIVE_ENV_FILE"):
    load_dotenv(Path(env_file))


def _skip_without_live_sandbox() -> None:
    if os.environ.get("SENTINEL_AI_LIVE_SANDBOX_TESTS") != "1":
        pytest.skip("set SENTINEL_AI_LIVE_SANDBOX_TESTS=1 to create a real sandbox")
    if not os.environ.get("HF_TOKEN"):
        pytest.skip("set HF_TOKEN to create a real sandbox")


def test_live_sandbox_authenticated_agent_communication():
    _skip_without_live_sandbox()

    token = os.environ["HF_TOKEN"]
    owner = HfApi(token=token).whoami()["name"]
    sandbox = None

    try:
        sandbox = Sandbox.create(
            owner=owner,
            name="sentinel-ai-live-auth",
            hardware="cpu-basic",
            private=True,
            token=token,
            secrets={"HF_TOKEN": token},
            wait_timeout=900,
        )

        unauthenticated = httpx.Client(
            base_url=sandbox._base_url,
            timeout=30,
            follow_redirects=True,
        )
        try:
            denied = unauthenticated.post("exists", json={"path": "/tmp"})
            assert denied.status_code in {
                401,
                403,
                404,
            }  # HF private-Space edge may 404 to avoid leaking existence
        finally:
            unauthenticated.close()

        bash = sandbox.bash("printf sandbox-live-ok", timeout=30)
        assert bash.success, bash.error
        assert "sandbox-live-ok" in bash.output

        write = sandbox.write("/tmp/sentinel_ai_live_auth.txt", "alpha\nbeta\n")
        assert write.success, write.error

        exists = sandbox._call("exists", {"path": "/tmp/sentinel_ai_live_auth.txt"})
        assert exists.success, exists.error
        assert exists.output == "true"

        read = sandbox.read("/tmp/sentinel_ai_live_auth.txt")
        assert read.success, read.error
        assert "alpha" in read.output
        assert "beta" in read.output

        reattached = Sandbox.connect(
            sandbox.space_id,
            token=token,
            api_token=sandbox.api_token,
        )
        try:
            reread = reattached.read("/tmp/sentinel_ai_live_auth.txt")
            assert reread.success, reread.error
            assert "alpha" in reread.output
        finally:
            reattached._client.close()
    finally:
        if sandbox is not None:
            sandbox.delete()
