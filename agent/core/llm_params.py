"""LiteLLM kwargs resolution for the model ids this agent accepts."""

import os

from agent.core.local_models import (
    LOCAL_MODEL_API_KEY_DEFAULT,
    LOCAL_MODEL_API_KEY_ENV,
    LOCAL_MODEL_BASE_URL_ENV,
    is_reserved_local_model_id,
    local_model_name,
    local_model_provider,
)
from agent.core.model_ids import (
    strip_platformops_model_prefix,
)


class UnsupportedEffortError(ValueError):
    """The requested effort isn't valid for this provider's API surface."""


def _local_api_base(base_url: str) -> str:
    base = base_url.strip().rstrip("/")
    if base.endswith("/v1"):
        return base
    return f"{base}/v1"


def _resolve_local_model_params(
    model_name: str,
    reasoning_effort: str | None = None,
    strict: bool = False,
) -> dict:
    if reasoning_effort and strict:
        raise UnsupportedEffortError(
            "Local OpenAI-compatible endpoints don't accept reasoning_effort"
        )

    local_name = local_model_name(model_name)
    if local_name is None:
        raise ValueError(f"Unsupported local model id: {model_name}")

    provider = local_model_provider(model_name)
    assert provider is not None
    raw_base = (
        os.environ.get(provider["base_url_env"])
        or os.environ.get(LOCAL_MODEL_BASE_URL_ENV)
        or provider["base_url_default"]
    )
    api_key = (
        os.environ.get(provider["api_key_env"])
        or os.environ.get(LOCAL_MODEL_API_KEY_ENV)
        or LOCAL_MODEL_API_KEY_DEFAULT
    )
    return {
        "model": f"openai/{local_name}",
        "api_base": _local_api_base(raw_base),
        "api_key": api_key,
    }


def _is_nim_model(model_id: str) -> bool:
    return model_id.startswith("nvidia/")

def _resolve_llm_params(
    model_name: str,
    session_token: str | None = None,
    reasoning_effort: str | None = None,
    strict: bool = False,
) -> dict:
    normalized_model = strip_platformops_model_prefix(model_name) or model_name

    if is_reserved_local_model_id(normalized_model):
        raise ValueError(f"Unsupported local model id: {normalized_model}")

    if local_model_provider(normalized_model) is not None:
        return _resolve_local_model_params(normalized_model, reasoning_effort, strict)

    # NVIDIA NIM models use the nvidia_nim/ provider prefix in LiteLLM
    if _is_nim_model(normalized_model):
        nim_key = os.environ.get("NVIDIA_NIM_API_KEY")
        if not nim_key:
            nim_key = "sk-nim-no-key"
        return {
            "model": f"nvidia_nim/{normalized_model.removeprefix('nvidia/')}",
            "api_key": nim_key,
        }

    params = {
        "model": f"openai/{normalized_model}",
        "api_key": session_token,
    }
    if reasoning_effort:
        params["extra_body"] = {"reasoning_effort": reasoning_effort}
    return params
