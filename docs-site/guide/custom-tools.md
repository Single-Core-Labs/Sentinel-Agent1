# Building Custom Tools

Sentinel exposes a deterministic, zero-cost tool surface plus an extensible
registry. Tools split into three kinds:

- **Built-in tools** registered by the runtime (bash, read, write, edit, grep,
  glob, git, web search, research, docs, plan, subagent, notify, GitHub tools).
- **GPU tools** from `sentinel-gpu-profiler` (profile/emulate/bench) — all
  deterministic, no LLM token spend.
- **Plugins** — directories containing a `sentinel-plugin.toml` manifest that
  register extra tools and policy hooks.

## Installing a plugin

```bash
sentinel plugin install <dir>      # from a local directory
sentinel plugin list               # list installed plugins
sentinel plugin remove <id>        # remove a plugin
```

A plugin directory contains:

```toml
# sentinel-plugin.toml
id = "policy-guard"
name = "Policy Guard"
version = "0.1.0"
```

At load time the CLI prints how many plugins were loaded (`✓ N plugins loaded`)
and surfaces each failing plugin (`✖ M plugins failed`) instead of silently
skipping it.

## Policy hooks

External policy scripts gate every tool call. Pass the command on the CLI:

```bash
sentinel ai --hook-command <cmd>
```

The script receives the tool invocation on stdout and must reply with one of:

- `allow`
- `deny <reason>`
- `ask`

The gate is fail-closed: anything else blocks the tool call.

## GPU tools (zero-cost)

Bundle in `sentinel-gpu-profiler`:

| Command | What it does |
|---|---|
| `/gpu ps` | Running GPU processes |
| `/gpu detailed` | Full `nvidia-smi -q` |
| `/profile <file>` | Analyze kernel source (CUDA/Triton/Mojo/Numba/PyTorch/CUTE) with block-size recommendations |
| `/profile dmon <sec>` | Real-time `nvidia-smi dmon` with anomaly detection |
| `/profile benchmark <file>` | Compile and run a kernel with `nvcc` |
| `/emulate <file> --all` | Run the GPU emulator on 10 architectures |
| `/bench kernel <file>` | Auto-sweep block sizes with heuristic scoring |

## Writing tools in Rust

Tools are registered in `sentinel_tools::ToolRegistry`. Implement the
`sentinel_tools::Tool` trait (name, description, and an async `execute`), then
`tool_registry.register(tool)`. New commands bound to the GPU profiler live in
`sentinel-gpu-profiler` — add a language analyzer in `langs.rs` and wire it in
`cmd_profile`.