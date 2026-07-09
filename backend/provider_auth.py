"""Multi-provider authentication registry.

Manages API keys and auth state for LLM providers (Google AI Studio,
Anthropic, OpenAI, etc.) so users can bring their own API keys or
log in via OAuth directly — no gateway required.
"""

import logging
import os
import secrets
import time
from dataclasses import dataclass, field
from typing import Any, Literal

import httpx

logger = logging.getLogger(__name__)

# ── Provider definitions ─────────────────────────────────────────────

ProviderAuthType = Literal["api_key", "oauth", "env_only"]


@dataclass
class ProviderModel:
    id: str
    name: str
    provider: str
    description: str = ""
    tag: str = ""


@dataclass
class ProviderInfo:
    """Static metadata about a model provider."""

    id: str
    name: str
    auth_type: ProviderAuthType
    docs_url: str = ""
    base_url: str = ""
    api_key_instructions: str = ""
    oauth_client_id_env: str = ""
    oauth_authorize_url: str = ""
    oauth_token_url: str = ""
    oauth_scopes: list[str] = field(default_factory=list)
    models: list[ProviderModel] = field(default_factory=list)


@dataclass
class ProviderModel:
    """A model available from a specific provider."""

    provider_id: str
    model_id: str
    name: str
    description: str = ""
    tag: str = ""


@dataclass
class ProviderCredential:
    provider_id: str
    api_key: str = ""
    base_url: str = ""
    oauth_token: str = ""
    oauth_refresh_token: str = ""
    expires_at: float = 0.0
    verified: bool = False


# ── OAuth state store (in-memory, 5-min TTL) ─────────────────────────

_OAUTH_STATE_TTL = 300
_oauth_states: dict[str, dict] = {}


def _cleanup_expired_states() -> None:
    now = time.time()
    expired = [k for k, v in _oauth_states.items() if now > v.get("expires_at", 0)]
    for k in expired:
        _oauth_states.pop(k, None)


# ── Provider registry ───────────────────────────────────────────────

PROVIDERS: dict[str, ProviderInfo] = {
    "google-ai-studio": ProviderInfo(
        id="google-ai-studio",
        name="Google AI Studio",
        auth_type="api_key",
        docs_url="https://aistudio.google.com/apikey",
        api_key_instructions="Get your API key at https://aistudio.google.com/apikey",
        base_url="https://generativelanguage.googleapis.com/v1beta",
        models=[
            ProviderModel("google-ai-studio", "gemini-2.5-pro", "Gemini 2.5 Pro", "Best reasoning, large context, multimodal", "large-ctx"),
            ProviderModel("google-ai-studio", "gemini-2.5-flash", "Gemini 2.5 Flash", "Fast, cost-efficient, multimodal", "fast"),
            ProviderModel("google-ai-studio", "gemini-2.0-flash", "Gemini 2.0 Flash", "Legacy flash model", "fast"),
        ],
    ),
    "anthropic": ProviderInfo(
        id="anthropic",
        name="Anthropic",
        auth_type="api_key",
        docs_url="https://console.anthropic.com/",
        api_key_instructions="Get your API key at https://console.anthropic.com/settings/keys",
        base_url="https://api.anthropic.com",
        models=[
            ProviderModel("anthropic", "claude-sonnet-4", "Claude Sonnet 4", "Best balance of speed and capability", "recommended"),
            ProviderModel("anthropic", "claude-opus-4.8:fal-ai", "Claude Opus 4.8 (via Fal)", "Most capable, complex reasoning", "powerful"),
            ProviderModel("anthropic", "claude-sonnet-4.5", "Claude Sonnet 4.5", "Latest Sonnet generation", "recommended"),
            ProviderModel("anthropic", "claude-haiku-3.5", "Claude Haiku 3.5", "Fast, lightweight", "fast"),
        ],
    ),
    "openai": ProviderInfo(
        id="openai",
        name="OpenAI",
        auth_type="api_key",
        docs_url="https://platform.openai.com/",
        api_key_instructions="Get your API key at https://platform.openai.com/api-keys",
        base_url="https://api.openai.com/v1",
        models=[
            ProviderModel("openai", "gpt-4o", "GPT-4o", "Fast multimodal, strong coding", "fast"),
            ProviderModel("openai", "gpt-4.5", "GPT-4.5", "Latest flagship model", "powerful"),
            ProviderModel("openai", "gpt-5.5", "GPT-5.5", "Next-gen reasoning model", "powerful"),
            ProviderModel("openai", "o3-mini", "o3-mini", "Fast reasoning, smaller context", "fast"),
            ProviderModel("openai", "o1-pro", "o1 Pro", "Deep reasoning, high cost", "powerful"),
        ],
    ),
    "deepseek": ProviderInfo(
        id="deepseek",
        name="DeepSeek",
        auth_type="api_key",
        docs_url="https://platform.deepseek.com/",
        api_key_instructions="Get your API key at https://platform.deepseek.com/api_keys",
        base_url="https://api.deepseek.com",
        models=[
            ProviderModel("deepseek", "deepseek-chat-v4", "DeepSeek V4 Chat", "Open-weight, strong reasoning", "open"),
            ProviderModel("deepseek", "deepseek-reasoner", "DeepSeek R1", "Deep reasoning model", "open"),
        ],
    ),
    "github-copilot": ProviderInfo(
        id="github-copilot",
        name="GitHub Copilot",
        auth_type="oauth",
        docs_url="https://github.com/settings/tokens",
        oauth_client_id_env="GITHUB_COPILOT_CLIENT_ID",
        oauth_authorize_url="https://github.com/login/oauth/authorize",
        oauth_token_url="https://github.com/login/oauth/access_token",
        oauth_scopes=["read:user", "copilot"],
        models=[
            ProviderModel("github-copilot", "copilot-gpt-4o", "Copilot GPT-4o", "GitHub Copilot hosted model", "copilot"),
            ProviderModel("github-copilot", "copilot-claude-sonnet", "Copilot Claude Sonnet", "GitHub Copilot with Claude", "copilot"),
        ],
    ),
    "chatgpt-plus": ProviderInfo(
        id="chatgpt-plus",
        name="ChatGPT Plus/Pro",
        auth_type="oauth",
        docs_url="https://platform.openai.com/",
        oauth_client_id_env="OPENAI_OAUTH_CLIENT_ID",
        oauth_authorize_url="https://platform.openai.com/oauth/authorize",
        oauth_token_url="https://platform.openai.com/oauth/token",
        oauth_scopes=["openid", "profile", "email"],
        models=[
            ProviderModel("chatgpt-plus", "gpt-4o", "GPT-4o (Plus)", "ChatGPT Plus tier model", "fast"),
            ProviderModel("chatgpt-plus", "gpt-4.5", "GPT-4.5 (Plus)", "ChatGPT Plus flagship", "powerful"),
        ],
    ),
    "models-dev": ProviderInfo(
        id="models-dev",
        name="Models.dev",
        auth_type="api_key",
        docs_url="https://models.dev",
        api_key_instructions="Get your models.dev API key at https://models.dev/keys",
        base_url="https://api.models.dev/v1",
        models=[
            ProviderModel("models-dev", "models-dev/gpt-4o", "GPT-4o", "Via models.dev routing", "fast"),
            ProviderModel("models-dev", "models-dev/claude-sonnet", "Claude Sonnet", "Via models.dev routing", "recommended"),
        ],
    ),
}


def get_providers() -> list[dict[str, Any]]:
    return [
        {
            "id": p.id,
            "name": p.name,
            "auth_type": p.auth_type,
            "docs_url": p.docs_url,
            "api_key_instructions": p.api_key_instructions,
            "models": [
                {
                    "provider_id": m.provider_id,
                    "model_id": m.model_id,
                    "name": m.name,
                    "description": m.description,
                    "tag": m.tag,
                }
                for m in p.models
            ],
        }
        for p in PROVIDERS.values()
    ]


# ── Credential store (in-memory, per-user) ───────────────────────────

# user_id -> { provider_id -> ProviderCredential }
_credential_store: dict[str, dict[str, ProviderCredential]] = {}


def get_user_credentials(user_id: str) -> dict[str, dict[str, Any]]:
    creds = _credential_store.get(user_id, {})
    return {
        pid: {
            "provider_id": c.provider_id,
            "has_api_key": bool(c.api_key),
            "has_oauth_token": bool(c.oauth_token),
            "verified": c.verified,
            "base_url": c.base_url or "",
        }
        for pid, c in creds.items()
    }


def get_provider_credential(user_id: str, provider_id: str) -> ProviderCredential | None:
    return _credential_store.get(user_id, {}).get(provider_id)


def set_provider_api_key(user_id: str, provider_id: str, api_key: str, base_url: str = "") -> None:
    if user_id not in _credential_store:
        _credential_store[user_id] = {}
    existing = _credential_store[user_id].get(provider_id)
    if existing:
        existing.api_key = api_key
        existing.base_url = base_url or existing.base_url
        existing.verified = False
    else:
        _credential_store[user_id][provider_id] = ProviderCredential(
            provider_id=provider_id,
            api_key=api_key,
            base_url=base_url,
        )


def set_provider_oauth_token(user_id: str, provider_id: str, token: str, refresh_token: str = "", expires_in: int = 3600) -> None:
    if user_id not in _credential_store:
        _credential_store[user_id] = {}
    _credential_store[user_id][provider_id] = ProviderCredential(
        provider_id=provider_id,
        oauth_token=token,
        oauth_refresh_token=refresh_token,
        expires_at=time.time() + expires_in,
    )


def mark_credential_verified(user_id: str, provider_id: str) -> None:
    creds = _credential_store.get(user_id, {}).get(provider_id)
    if creds:
        creds.verified = True


def remove_credential(user_id: str, provider_id: str) -> None:
    creds = _credential_store.get(user_id)
    if creds:
        creds.pop(provider_id, None)


# ── OAuth helpers ────────────────────────────────────────────────────


def create_oauth_state(provider_id: str, redirect_uri: str) -> str:
    _cleanup_expired_states()
    state = secrets.token_urlsafe(32)
    _oauth_states[state] = {
        "provider_id": provider_id,
        "redirect_uri": redirect_uri,
        "expires_at": time.time() + _OAUTH_STATE_TTL,
    }
    return state


def consume_oauth_state(state: str) -> dict[str, str] | None:
    data = _oauth_states.pop(state, None)
    if data is None:
        return None
    if time.time() > data.get("expires_at", 0):
        return None
    return data


def build_oauth_authorize_url(provider_id: str, redirect_uri: str, request_uri: str) -> str | None:
    provider = PROVIDERS.get(provider_id)
    if not provider or provider.auth_type != "oauth":
        return None
    from urllib.parse import urlencode

    client_id = os.environ.get(provider.oauth_client_id_env, "")
    if not client_id:
        logger.warning("OAuth client ID not set for provider %s (env: %s)", provider_id, provider.oauth_client_id_env)
        return None
    state = create_oauth_state(provider_id, redirect_uri)
    params = {
        "client_id": client_id,
        "redirect_uri": redirect_uri,
        "scope": " ".join(provider.oauth_scopes),
        "response_type": "code",
        "state": state,
    }
    return f"{provider.oauth_authorize_url}?{urlencode(params)}"


async def exchange_oauth_code(provider_id: str, code: str, redirect_uri: str) -> dict[str, Any] | None:
    provider = PROVIDERS.get(provider_id)
    if not provider or provider.auth_type != "oauth":
        return None

    client_id = os.environ.get(provider.oauth_client_id_env, "")
    client_secret_env = f"{provider.oauth_client_id_env}_SECRET"
    client_secret = os.environ.get(client_secret_env, "")

    async with httpx.AsyncClient() as client:
        try:
            response = await client.post(
                provider.oauth_token_url,
                data={
                    "grant_type": "authorization_code",
                    "code": code,
                    "redirect_uri": redirect_uri,
                    "client_id": client_id,
                    "client_secret": client_secret,
                },
            )
            response.raise_for_status()
            token_data = response.json()
            return {
                "access_token": token_data.get("access_token"),
                "refresh_token": token_data.get("refresh_token", ""),
                "expires_in": token_data.get("expires_in", 3600),
            }
        except httpx.HTTPError as e:
            logger.warning("OAuth token exchange failed for %s: %s", provider_id, e)
            return None


# ── Provider health check ────────────────────────────────────────────


async def check_provider_health(provider_id: str, credential: ProviderCredential) -> dict[str, Any]:
    """Ping a provider's API to verify credentials and check status."""
    provider = PROVIDERS.get(provider_id)
    if not provider:
        return {"status": "error", "error": f"Unknown provider: {provider_id}"}

    api_key = credential.api_key or credential.oauth_token
    if not api_key:
        return {"status": "error", "error": "No credentials configured"}

    base_url = credential.base_url or provider.base_url

    try:
        if provider_id == "google-ai-studio":
            url = f"{base_url}/models?key={api_key}"
        elif provider_id == "anthropic":
            url = f"{base_url}/v1/messages"
            headers = {"x-api-key": api_key, "anthropic-version": "2023-06-01"}
        elif provider_id == "openai" or provider_id == "chatgpt-plus":
            url = f"{base_url}/models"
            headers = {"Authorization": f"Bearer {api_key}"}
        elif provider_id == "deepseek":
            url = f"{base_url}/v1/models"
            headers = {"Authorization": f"Bearer {api_key}"}
        elif provider_id == "models-dev":
            url = f"{base_url}/models"
            headers = {"Authorization": f"Bearer {api_key}"}
        elif provider_id == "github-copilot":
            url = "https://api.github.com/user"
            headers = {"Authorization": f"Bearer {api_key}", "Accept": "application/vnd.github+json"}
        else:
            return {"status": "skipped", "error": f"No health check implemented for {provider_id}"}

        async with httpx.AsyncClient(timeout=10.0) as client:
            resp = await client.get(url, headers=headers if provider_id != "google-ai-studio" else None)

        if resp.status_code in (200, 201):
            return {"status": "ok", "provider_id": provider_id}
        elif resp.status_code == 401:
            return {"status": "error", "error_type": "auth", "error": "Invalid or expired API key"}
        elif resp.status_code == 403:
            return {"status": "error", "error_type": "credits", "error": "Insufficient credits or access denied"}
        elif resp.status_code == 429:
            return {"status": "error", "error_type": "rate_limit", "error": "Rate limited"}
        else:
            return {"status": "error", "error_type": "unknown", "error": f"HTTP {resp.status_code}: {resp.text[:200]}"}

    except httpx.TimeoutException:
        return {"status": "error", "error_type": "network", "error": "Connection timed out"}
    except httpx.HTTPError as e:
        return {"status": "error", "error_type": "network", "error": str(e)}