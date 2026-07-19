"""LiteLLM kwargs resolution for the model ids this agent accepts."""

import logging
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
    strip_sentinel_ai_model_prefix,
)

logger = logging.getLogger(__name__)

# Provider API key mapping: provider_id -> env var name
DIRECT_PROVIDER_API_KEYS: dict[str, str] = {
    "anthropic": "ANTHROPIC_API_KEY",
    "google-ai-studio": "GOOGLE_AI_STUDIO_API_KEY",
    "openai": "OPENAI_API_KEY",
    "deepseek": "DEEPSEEK_API_KEY",
    "models-dev": "MODELS_DEV_API_KEY",
    "github-copilot": "GITHUB_COPILOT_TOKEN",
    "chatgpt-plus": "OPENAI_API_KEY",
}

# Direct provider base URLs
DIRECT_PROVIDER_BASE_URLS: dict[str, str] = {
    "anthropic": "https://api.anthropic.com",
    "google-ai-studio": "https://generativelanguage.googleapis.com/v1beta",
    "openai": "https://api.openai.com/v1",
    "deepseek": "https://api.deepseek.com",
    "models-dev": "https://api.models.dev/v1",
}

# Direct provider LiteLLM prefixes
DIRECT_PROVIDER_LITELLM_PREFIXES: dict[str, str] = {
    "anthropic": "anthropic/",
    "openai": "openai/",
    "deepseek": "openai/",
    "google-ai-studio": "gemini/",
    "models-dev": "openai/",
}

# LiteLLM custom provider prefixes for routing
LITELLM_PROVIDER_PREFIXES: dict[str, str] = {
    "anthropic": "anthropic/",
    "openai": "openai/",
    "deepseek": "openai/",
    "google-ai-studio": "gemini/",
    "nvidia": "nvidia_nim/",
}

# Map LiteLLM provider prefix to our internal provider_id for direct auth
LITELLM_PREFIX_TO_PROVIDER: dict[str, str] = {
    "anthropic/": "anthropic",
    "openai/": "openai",
    "gemini/": "google-ai-studio",
}


def detect_provider_from_model_id(model_id: str) -> str | None:
    """Try to detect which provider a model id belongs to."""
    for prefix, provider_id in LITELLM_PREFIX_TO_PROVIDER.items():
        if model_id.startswith(prefix):
            return provider_id
    # Check direct provider model IDs (no prefix)
    if model_id.startswith("gemini-") or model_id.startswith("models/"):
        return "google-ai-studio"
    if model_id.startswith("claude-"):
        return "anthropic"
    if model_id.startswith("gpt-") or model_id.startswith("o"):
        return "openai"
    if model_id.startswith("deepseek-"):
        return "deepseek"
    return None


def resolve_direct_api_key(provider_id: str | None = None, model_id: str | None = None) -> str | None:
    """Resolve a direct API key for a provider from env vars."""
    if not provider_id and model_id:
        provider_id = detect_provider_from_model_id(model_id)
    if provider_id:
        env_var = DIRECT_PROVIDER_API_KEYS.get(provider_id)
        if env_var:
            key = os.environ.get(env_var)
            if key:
                return key
    return None


def resolve_direct_provider_params(
    model_name: str,
    provider_id: str | None = None,
    api_key: str | None = None,
) -> dict | None:
    """Resolve LiteLLM params for a direct provider call (no gateway)."""
    if not provider_id:
        provider_id = detect_provider_from_model_id(model_name)
    if not provider_id:
        return None

    actual_key = api_key or resolve_direct_api_key(provider_id)
    if not actual_key:
        logger.info("No direct API key found for %s, falling through to gateway", provider_id)
        return None

    # Strip the provider prefix from model name for LiteLLM
    for prefix in LITELLM_PREFIX_TO_PROVIDER:
        if model_name.startswith(prefix):
            model_name = model_name[len(prefix):]
            break

    litellm_prefix = DIRECT_PROVIDER_LITELLM_PREFIXES.get(provider_id)
    if not litellm_prefix:
        return None

    base_url = DIRECT_PROVIDER_BASE_URLS.get(provider_id)
    params = {
        "model": f"{litellm_prefix}{model_name}",
        "api_key": actual_key,
    }
    if base_url:
        params["api_base"] = base_url

    logger.debug("Resolved direct provider params for %s: model=%s", provider_id, params["model"])
    return params


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
    provider_api_key: str | None = None,
    provider_id: str | None = None,
) -> dict:
    normalized_model = strip_sentinel_ai_model_prefix(model_name) or model_name

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

    # Try direct provider auth first (no gateway)
    direct_params = resolve_direct_provider_params(
        model_name,
        provider_id=provider_id,
        api_key=provider_api_key,
    )
    if direct_params:
        if reasoning_effort:
            direct_params["extra_body"] = {"reasoning_effort": reasoning_effort}
        return direct_params

    # Fallback: route through the gateway/router with session token
    params = {
        "model": f"openai/{normalized_model}",
        "api_key": session_token,
    }
    if reasoning_effort:
        params["extra_body"] = {"reasoning_effort": reasoning_effort}
    return params
