CLAUDE_OPUS_48_MODEL_ID = "anthropic/claude-opus-4.8:fal-ai"
GPT_55_MODEL_ID = "openai/gpt-5.5:fal-ai"
KIMI_K27_CODE_MODEL_ID = "moonshotai/Kimi-K2.7-Code:novita"
MINIMAX_M3_MODEL_ID = "MiniMaxAI/MiniMax-M3:novita"
GLM_52_MODEL_ID = "zai-org/GLM-5.2:novita"
DEEPSEEK_V4_PRO_MODEL_ID = "deepseek-ai/DeepSeek-V4-Pro:novita"
NVIDIA_NEMOTRON_70B_MODEL_ID = "nvidia/llama-3.1-nemotron-70b-instruct"
NVIDIA_NEMOTRON_SUPER_49B_MODEL_ID = "nvidia/llama-3.3-nemotron-super-49b"
NVIDIA_NEMOTRON_340B_MODEL_ID = "nvidia/nemotron-4-340b-instruct"

# Direct provider model IDs (no gateway required)
CLAUDE_SONNET_4_MODEL_ID = "claude-sonnet-4"
CLAUDE_HAIKU_35_MODEL_ID = "claude-haiku-3.5"
GEMINI_25_PRO_MODEL_ID = "gemini/gemini-2.5-pro"
GEMINI_25_FLASH_MODEL_ID = "gemini/gemini-2.5-flash"
GPT_4O_MODEL_ID = "gpt-4o"
DEEPSEEK_CHAT_V4_MODEL_ID = "deepseek-chat-v4"

HOSTED_MODEL_IDS = {
    CLAUDE_OPUS_48_MODEL_ID,
    GPT_55_MODEL_ID,
    KIMI_K27_CODE_MODEL_ID,
    MINIMAX_M3_MODEL_ID,
    GLM_52_MODEL_ID,
    DEEPSEEK_V4_PRO_MODEL_ID,
    NVIDIA_NEMOTRON_70B_MODEL_ID,
    NVIDIA_NEMOTRON_SUPER_49B_MODEL_ID,
    NVIDIA_NEMOTRON_340B_MODEL_ID,
}

# Model IDs that work with direct provider auth (API key from env or input)
DIRECT_PROVIDER_MODEL_IDS: set[str] = {
    CLAUDE_SONNET_4_MODEL_ID,
    CLAUDE_HAIKU_35_MODEL_ID,
    GEMINI_25_PRO_MODEL_ID,
    GEMINI_25_FLASH_MODEL_ID,
    GPT_4O_MODEL_ID,
    DEEPSEEK_CHAT_V4_MODEL_ID,
}


def is_direct_provider_model_id(model_id: str | None) -> bool:
    if not model_id:
        return False
    return model_id in DIRECT_PROVIDER_MODEL_IDS


def strip_sentinel_ai_model_prefix(model_id: str | None) -> str | None:
    """Return model ids without LiteLLM's optional ``sentinel-ai/`` prefix."""
    if not model_id:
        return model_id
    return model_id.removeprefix("sentinel-ai/")


def is_known_router_model_id(model_id: str | None) -> bool:
    normalized = strip_sentinel_ai_model_prefix(model_id)
    return bool(normalized and normalized in HOSTED_MODEL_IDS)
