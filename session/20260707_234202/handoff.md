# Handoff

## Resume From Here
The `sentinel-ai` CLI should now start without crashing. OTel imports are optional, all env var references in the config JSON have defaults, the substitution function returns `""` instead of raising errors, and MCP servers with empty env vars are dropped at load time.

## Next Actions
- User to test `sentinel-ai` from their own PowerShell/cmd window (not through the bash tool — prompt_toolkit needs a real console)

## Watch Outs
- The bash tool has no real Windows console — `PromptSession()` will crash with `NoConsoleScreenBufferError`. Always test CLI interactively from user's own terminal.
- The `sentinel-ai` uv tool may need `uv tool install --reinstall --name sentinel-ai .` if the tool environment is stale.
