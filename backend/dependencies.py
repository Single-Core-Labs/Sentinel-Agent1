"""Authentication dependencies for FastAPI routes.

- In dev mode (OAUTH_CLIENT_ID not set): auth is bypassed, returns a default "dev" user.
- In production: validates Bearer tokens or cookies against HF OAuth.
"""

import logging
import os
import time
from collections.abc import Iterable
from hashlib import sha256
from typing import Any

import httpx
from fastapi import HTTPException, Request, status



logger = logging.getLogger(__name__)

OPENID_PROVIDER_URL = os.environ.get("OPENID_PROVIDER_URL", "https://platformops.co")
AUTH_ENABLED = bool(os.environ.get("OAUTH_CLIENT_ID", ""))

# Simple in-memory token cache: token -> (user_info, expiry_time)
_token_cache: dict[str, tuple[dict[str, Any], float]] = {}
TOKEN_CACHE_TTL = 300  # 5 minutes

DEV_USER: dict[str, Any] = {
    "user_id": "dev",
    "username": "dev",
    "authenticated": True,
    "plan": "pro",
}

OAUTH_SCOPE_COOKIE = "oauth_scope_hash"
REQUIRED_OAUTH_SCOPES: tuple[str, ...] = (
    "openid",
    "profile",
    "read-billing",
    "read-repos",
    "write-repos",
    "contribute-repos",
    "manage-repos",
    "write-collections",
    "inference-api",
    "jobs",
    "write-discussions",
)

def normalize_oauth_scopes(scopes: Iterable[str]) -> tuple[str, ...]:
    """Return stable, de-duplicated OAuth scopes preserving declaration order."""
    seen: set[str] = set()
    normalized: list[str] = []
    for scope in scopes:
        value = str(scope).strip()
        if not value or value in seen:
            continue
        seen.add(value)
        normalized.append(value)
    return tuple(normalized)


def configured_oauth_scopes() -> tuple[str, ...]:
    """Return the scopes this backend should request from OAuth."""
    env_scopes = os.environ.get("OAUTH_SCOPES", "").split()
    return normalize_oauth_scopes((*env_scopes, *REQUIRED_OAUTH_SCOPES))


def oauth_scope_fingerprint(scopes: Iterable[str] | None = None) -> str:
    """Return a non-secret fingerprint for the current OAuth scope contract."""
    scope_list = configured_oauth_scopes() if scopes is None else scopes
    payload = " ".join(sorted(normalize_oauth_scopes(scope_list)))
    return sha256(payload.encode("utf-8")).hexdigest()[:16]


def _cookie_has_current_oauth_scope_marker(request: Request) -> bool:
    return request.cookies.get(OAUTH_SCOPE_COOKIE) == oauth_scope_fingerprint()


async def _validate_token(token: str) -> dict[str, Any] | None:
    """Validate a token against HF OAuth userinfo endpoint.

    Results are cached for TOKEN_CACHE_TTL seconds to avoid excessive API calls.
    """
    now = time.time()

    # Check cache
    if token in _token_cache:
        user_info, expiry = _token_cache[token]
        if now < expiry:
            return user_info
        del _token_cache[token]

    # Validate against HF
    async with httpx.AsyncClient(timeout=10.0) as client:
        try:
            response = await client.get(
                f"{OPENID_PROVIDER_URL}/oauth/userinfo",
                headers={"Authorization": f"Bearer {token}"},
            )
            if response.status_code != 200:
                logger.debug("Token validation failed: status %d", response.status_code)
                return None
            user_info = response.json()
            _token_cache[token] = (user_info, now + TOKEN_CACHE_TTL)
            return user_info
        except httpx.HTTPError as e:
            logger.warning("Token validation error: %s", e)
            return None


def _user_from_info(user_info: dict[str, Any]) -> dict[str, Any]:
    """Build a normalized user dict from HF userinfo response."""
    return {
        "user_id": user_info.get("sub", user_info.get("preferred_username", "unknown")),
        "username": user_info.get("preferred_username", "unknown"),
        "name": user_info.get("name"),
        "picture": user_info.get("picture"),
        "authenticated": True,
    }


async def _extract_user_from_token(token: str) -> dict[str, Any] | None:
    """Validate a token and return a user dict, or None."""
    user_info = await _validate_token(token)
    if user_info is None:
        return None
    user = _user_from_info(user_info)
    user["plan"] = "free"
    return user


def bearer_token_from_header(auth_header: str) -> str | None:
    """Extract bearer token from Authorization header."""
    if not auth_header:
        return None
    parts = auth_header.split()
    if len(parts) != 2 or parts[0].lower() != "bearer":
        return None
    return parts[1]


async def _dev_user_from_env() -> dict[str, Any]:
    return dict(DEV_USER)


async def get_current_user(request: Request) -> dict[str, Any]:
    """FastAPI dependency: extract and validate the current user.

    Checks (in order):
    1. Authorization: Bearer <token> header
    2. access_token cookie

    In dev mode (AUTH_ENABLED=False), returns the dev user.
    """
    if not AUTH_ENABLED:
        return await _dev_user_from_env()

    # Bearer callers manage token lifecycle themselves; only browser cookie
    # auth is forced through the scope-freshness marker below.
    token = bearer_token_from_header(request.headers.get("Authorization", ""))
    if token:
        user = await _extract_user_from_token(token)
        if user:
            return user

    # Try cookie
    token = request.cookies.get("access_token")
    if token:
        if not _cookie_has_current_oauth_scope_marker(request):
            logger.info(
                "Rejecting stale OAuth cookie; current scopes require refresh."
            )
            raise HTTPException(
                status_code=status.HTTP_401_UNAUTHORIZED,
                detail="Authentication scopes changed. Please log in again.",
                headers={"WWW-Authenticate": "Bearer"},
            )
        user = await _extract_user_from_token(token)
        if user:
            return user

    raise HTTPException(
        status_code=status.HTTP_401_UNAUTHORIZED,
        detail="Not authenticated. Please log in via /auth/login.",
        headers={"WWW-Authenticate": "Bearer"},
    )
