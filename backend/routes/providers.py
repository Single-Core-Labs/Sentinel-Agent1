"""Provider authentication and management routes.

Users can bring their own API keys for any supported provider (Google AI Studio,
Anthropic, OpenAI, DeepSeek, etc.) or log in via OAuth (GitHub Copilot, ChatGPT Plus).
No gateway/router required — direct provider access.
"""

import logging
from typing import Any

from dependencies import get_current_user
from fastapi import APIRouter, Depends, HTTPException, Request
from fastapi.responses import RedirectResponse
from pydantic import BaseModel, Field

from provider_auth import (
    PROVIDERS,
    build_oauth_authorize_url,
    check_provider_health,
    consume_oauth_state,
    exchange_oauth_code,
    get_provider_credential,
    get_providers,
    get_user_credentials,
    mark_credential_verified,
    remove_credential,
    set_provider_api_key,
    set_provider_oauth_token,
)

logger = logging.getLogger(__name__)

router = APIRouter(prefix="/api/providers", tags=["providers"])


# ── Request/Response models ──────────────────────────────────────────


class SetApiKeyRequest(BaseModel):
    provider_id: str
    api_key: str = Field(..., min_length=1, max_length=4096)
    base_url: str = Field(default="", max_length=1024)


class ProviderStatusResponse(BaseModel):
    provider_id: str
    provider_name: str
    has_api_key: bool = False
    has_oauth: bool = False
    verified: bool = False
    auth_type: str = "api_key"


class ProviderListResponse(BaseModel):
    providers: list[dict[str, Any]]


# ── Routes ────────────────────────────────────────────────────────────


@router.get("")
async def list_providers() -> list[dict[str, Any]]:
    return get_providers()


@router.get("/status", response_model=dict[str, ProviderStatusResponse])
async def provider_status(user: dict = Depends(get_current_user)) -> dict:
    """Return auth status for all providers for the current user."""
    user_id = user.get("user_id", "dev")
    creds = get_user_credentials(user_id)
    result: dict[str, ProviderStatusResponse] = {}
    for pid, provider in PROVIDERS.items():
        c = creds.get(pid)
        result[pid] = ProviderStatusResponse(
            provider_id=pid,
            provider_name=provider.name,
            has_api_key=bool(c and c.get("has_api_key")),
            has_oauth=bool(c and c.get("has_oauth_token")),
            verified=bool(c and c.get("verified")),
            auth_type=provider.auth_type,
        )
    return result


@router.post("/keys")
async def set_api_key(
    req: SetApiKeyRequest,
    user: dict = Depends(get_current_user),
) -> dict[str, Any]:
    """Store an API key for a provider."""
    if req.provider_id not in PROVIDERS:
        raise HTTPException(status_code=404, detail=f"Unknown provider: {req.provider_id}")

    provider = PROVIDERS[req.provider_id]
    base_url = req.base_url or provider.base_url

    user_id = user.get("user_id", "dev")
    set_provider_api_key(user_id, req.provider_id, req.api_key, base_url)

    return {"status": "ok", "provider_id": req.provider_id, "message": f"API key saved for {provider.name}"}


@router.delete("/keys/{provider_id}")
async def remove_api_key(
    provider_id: str,
    user: dict = Depends(get_current_user),
) -> dict[str, str]:
    """Remove stored API key for a provider."""
    if provider_id not in PROVIDERS:
        raise HTTPException(status_code=404, detail=f"Unknown provider: {provider_id}")
    user_id = user.get("user_id", "dev")
    remove_credential(user_id, provider_id)
    return {"status": "ok", "provider_id": provider_id}


@router.post("/verify/{provider_id}")
async def verify_provider(
    provider_id: str,
    user: dict = Depends(get_current_user),
) -> dict[str, Any]:
    """Verify stored credentials by making a lightweight API call."""
    if provider_id not in PROVIDERS:
        raise HTTPException(status_code=404, detail=f"Unknown provider: {provider_id}")

    user_id = user.get("user_id", "dev")
    credential = get_provider_credential(user_id, provider_id)
    if not credential:
        raise HTTPException(status_code=400, detail=f"No credentials stored for {provider_id}")

    result = await check_provider_health(provider_id, credential)
    if result.get("status") == "ok":
        mark_credential_verified(user_id, provider_id)
    return result


@router.get("/oauth/login/{provider_id}")
async def oauth_login(
    provider_id: str,
    request: Request,
    user: dict = Depends(get_current_user),
) -> RedirectResponse:
    """Initiate OAuth flow for a provider (GitHub Copilot, ChatGPT Plus)."""
    if provider_id not in PROVIDERS:
        raise HTTPException(status_code=404, detail=f"Unknown provider: {provider_id}")

    provider = PROVIDERS[provider_id]
    if provider.auth_type != "oauth":
        raise HTTPException(status_code=400, detail=f"{provider.name} does not use OAuth")

    redirect_uri = str(request.url_for("oauth_callback"))
    auth_url = build_oauth_authorize_url(provider_id, redirect_uri, str(request.url))
    if not auth_url:
        raise HTTPException(
            status_code=500,
            detail=f"OAuth not configured for {provider.name}. Set {provider.oauth_client_id_env} env var.",
        )
    return RedirectResponse(url=auth_url)


@router.get("/oauth/callback")
async def oauth_callback(
    request: Request,
    code: str = "",
    state: str = "",
    user: dict = Depends(get_current_user),
) -> dict[str, Any]:
    """Handle OAuth callback from a provider."""
    stored = consume_oauth_state(state)
    if not stored:
        raise HTTPException(status_code=400, detail="Invalid or expired OAuth state")

    provider_id = stored["provider_id"]
    redirect_uri = stored["redirect_uri"]

    if not code:
        raise HTTPException(status_code=400, detail="No authorization code provided")

    token_data = await exchange_oauth_code(provider_id, code, redirect_uri)
    if not token_data:
        raise HTTPException(status_code=500, detail="Token exchange failed")

    access_token = token_data.get("access_token")
    if not access_token:
        raise HTTPException(status_code=500, detail="No access token returned")

    user_id = user.get("user_id", "dev")
    set_provider_oauth_token(
        user_id,
        provider_id,
        access_token,
        token_data.get("refresh_token", ""),
        token_data.get("expires_in", 3600),
    )

    return {
        "status": "ok",
        "provider_id": provider_id,
        "message": f"Successfully logged in to {PROVIDERS[provider_id].name}",
    }