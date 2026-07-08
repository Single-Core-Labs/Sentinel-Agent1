CLAUDE_OPUS_48_MODEL_ID = "anthropic/claude-opus-4.8:fal-ai"
GPT_55_MODEL_ID = "openai/gpt-5.5:fal-ai"
KIMI_K27_CODE_MODEL_ID = "moonshotai/Kimi-K2.7-Code:novita"
MINIMAX_M3_MODEL_ID = "MiniMaxAI/MiniMax-M3:novita"
GLM_52_MODEL_ID = "zai-org/GLM-5.2:novita"
DEEPSEEK_V4_PRO_MODEL_ID = "deepseek-ai/DeepSeek-V4-Pro:novita"
NVIDIA_NEMOTRON_70B_MODEL_ID = "nvidia/llama-3.1-nemotron-70b-instruct"
NVIDIA_NEMOTRON_SUPER_49B_MODEL_ID = "nvidia/llama-3.3-nemotron-super-49b"
NVIDIA_NEMOTRON_340B_MODEL_ID = "nvidia/nemotron-4-340b-instruct"

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


def strip_platformops_model_prefix(model_id: str | None) -> str | None:
    """Return model ids without LiteLLM's optional ``platformops/`` prefix."""
    if not model_id:
        return model_id
    return model_id.removeprefix("platformops/")


def is_known_router_model_id(model_id: str | None) -> bool:
    normalized = strip_platformops_model_prefix(model_id)
    return bool(normalized and normalized in HOSTED_MODEL_IDS)
