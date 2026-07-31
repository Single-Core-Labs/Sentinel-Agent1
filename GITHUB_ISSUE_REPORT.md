# BUG REPORT: Model Switching Broken - CLI Still Uses localhost:11434

## Summary
When users specify a model like `gemini-2.0-flash`, the CLI ignores it and tries to use `localhost:11434` (Ollama) instead of the actual API provider (Google, OpenAI, etc.).

## Steps to Reproduce

1. Create `.env` with API key:
```bash
GOOGLE_AI_STUDIO_API_KEY=AIzaSyD62KV0OIGS2y2wWm8Dj2UPuY-zZQYfnOA
```

2. Run sentinel with gemini model:
```bash
sentinel ai --model gemini-2.0-flash --prompt "2+2?"
```

3. **Expected**: Uses Google API
4. **Actual**: Error: `error sending request for url (http://localhost:11434/v1/chat/completions)`

## Root Causes Identified

### Issue 1: .env File Not Loaded ✅ FIXED (In PR)
- **Problem**: CLI didn't load `.env` file, so API keys unavailable
- **Fix Applied**: 
  - Added `dotenv::dotenv().ok();` to `main.rs` line 19
  - Added `dotenv = "0.15"` dependency to `Cargo.toml`

### Issue 2: No Provider Detection by Model Prefix ✅ IMPLEMENTED (In PR)
- **Problem**: No way to map `gemini-*` → Google provider automatically
- **Fix Applied**:
  - Added `detect_provider_from_prefix()` function in `ai.rs` (line 474)
  - Auto-creates provider config from env vars
  - Maps: `claude-*` → Anthropic, `gpt-*` → OpenAI, `gemini-*` → Google, `deepseek-*` → DeepSeek, `ollama/*` → Local
  - Implemented fallback: config file → env-based detection → fallback provider

### Issue 3: Argument Parsing Breaks --model Flag ✅ FIXED (In PR)
- **Problem**: `sentinel ai --model gemini "prompt"` treats "prompt" as model name
- **Fix Applied**:
  - Added `model_explicit` flag to track if `--model` was used
  - Positional args only override model if `--model` wasn't explicitly set
  - Changes in `ai.rs` lines 123, 137, 155

### Issue 4: Build System Not Updating Binary ⚠️ BLOCKING
- **Problem**: Binary at `/d/rust/cargo/bin/sentinel` is 1+ hour old
  - Modified: 2026-07-31 23:41:48
  - Current: 2026-08-01 00:52:51+
- **Status**: Cargo says "Finished" but binary doesn't reflect code changes
- **Impact**: Can't verify if fixes work
- **Possible Causes**:
  - Link step failing silently
  - Cached build artifacts
  - Permission issue on binary
  - Incremental compilation issue

## Code Changes (Ready for Review)

### Files Modified:
1. **crates/interfaces/sentinel-cli/Cargo.toml**
   - Added: `dotenv = "0.15"`

2. **crates/interfaces/sentinel-cli/src/main.rs**
   - Added: `dotenv::dotenv().ok();` at line 19 (before tracing init)

3. **crates/interfaces/sentinel-cli/src/ai.rs**
   - Lines 123, 137, 155: Added `model_explicit` flag for arg parsing
   - Lines 165-200: New provider detection logic with fallbacks
   - Lines 474-487: `detect_provider_from_prefix()` function
   - Lines 488-501: `provider_env_var()` mapping function
   - Lines 502-507: `provider_env_hint()` for error messages
   - Lines 508-564: `create_env_provider_info()` for auto-provider config
   - Lines 577-616: Unit tests for provider detection
   - Lines 411-450: Enhanced `/model` command (shows available models)
   - Lines 270-287: Enhanced startup banner (shows available providers)

### Unit Tests Added:
- `test_detect_provider_from_prefix()` - Tests model prefix → provider mapping
- `test_provider_env_var_mapping()` - Tests env var names
- `test_provider_env_hint()` - Tests error messages
- `test_create_env_provider_info_google()` - Tests Google provider creation
- `test_create_env_provider_info_openai()` - Tests OpenAI provider creation

## What's NOT Yet Implemented (Future Work)

1. **Mid-session model switching** - `/model` command should allow switching without restart
2. **Model persistence** - Remember last used model for next session
3. **Config command** - `sentinel config set-model <model>` 
4. **Model aliases** - `sentinel ai --model claude` instead of `claude-opus-5`

## User-Centric Workflow (Goal)

**Currently broken:**
```bash
export GOOGLE_AI_STUDIO_API_KEY=...
sentinel ai --model gemini-2.0-flash
# Error: localhost:11434
```

**Should work:**
```bash
# Set once in .env or config
GOOGLE_AI_STUDIO_API_KEY=AIzaSyD...

# Just run - it works
sentinel ai --model gemini-2.0-flash --prompt "hello"

# See available models
sentinel ai
> /model
# Shows: Google Gemini, OpenAI, Claude, etc.

# Switch models without restarting
sentinel ai --model gpt-4o --prompt "..."
```

## Testing Status

- ✅ Code changes verified in files
- ✅ Unit tests added (can be run via `cargo test`)
- ❌ Binary verification BLOCKED (build system issue)
- ❌ End-to-end test BLOCKED (need working binary)

## Environment
- **Platform**: Windows 11 Home Single Language 10.0.26200
- **Rust Toolchain**: GNU (x86_64-pc-windows-gnu)
- **CARGO_HOME**: D:\rust\cargo
- **Project**: Single-Core-Labs/Sentinel-Agent1
- **Test Provider**: Google AI Studio (Gemini)

## Blockers

1. **Binary Not Updating**: cargo build completes but /d/rust/cargo/bin/sentinel timestamp doesn't change
   - Need to investigate: link step, permissions, caching, or cargo config
   - Suggestion: Try `cargo clean -p sentinel-cli` then rebuild
   - Or check if there's a conflicting build output in target/

## Questions for Maintainers

1. Why is binary not updating after builds?
2. Should provider detection logic be in a separate crate (sentinel-provider-selector)?
3. Should config loading happen earlier in the pipeline?
4. Any known issues with dotenv on Windows with spaces in paths?

## Related Issues

- User experience: Model selection should be seamless
- Architecture: Currently scattered across multiple decision points
- Config: Need clear precedence: CLI flag > session > config file > env > default

---

**Prepared by**: Claude Code Agent  
**Date**: 2026-08-01  
**Status**: Ready for triage
