# Development Containers

Sentinel AI provides two Dev Container profiles:

| Profile | Config | Use case |
|---|---|---|
| **Standard** | `devcontainer.json` | Active development, full tooling |
| **Secure** | `devcontainer.secure.json` | Network-restricted, sandboxed runtime |

Both profiles share a common `Dockerfile` that pre-installs:

- Rust toolchain (stable, profile=complete) with `clippy`, `rustfmt`, `rust-analyzer`
- musl cross-compilation targets (`x86_64`, `aarch64`)
- Bazelisk (Bazel launcher)
- Python 3.12 + pipx
- Node.js 22 + npm
- bubblewrap (setuid in secure profile)
- iptables + ipset (for secure firewall)
- C/C++ build toolchain + musl-tools

---

## Standard Profile

```bash
# Build and open in VS Code
devcontainer open --workspace-folder .

# Or build and start headless
devcontainer up --workspace-folder .
devcontainer exec --workspace-folder . cargo test
```

### What it does

1. Builds the `Dockerfile` with all tooling.
2. Runs setup to:
   - Set up persistent shell history (mounted volume).
   - Configure Git (autocrlf=input, pull.rebase, safe.directory, LFS).
   - Setup Cargo config (git-fetch-with-cli, sparse protocol).
   - Validate Rust components (clippy, rustfmt, rust-analyzer).
   - Add musl cross-compilation targets.
   - Install pre-built Sentinel CLI binary if found.
3. Mounts persistent volumes for bash history and Cargo registry cache.

---

## Secure Profile

The secure profile adds strict network isolation and process sandboxing:

- **Default-deny outbound firewall** via iptables + ipset.
- **IPv6 fully blocked** to prevent protocol bypass.
- **Bubblewrap setuid** for sandboxed agent execution.
- **Seccomp=unconfined** — required for bubblewrap to establish the inner sandbox.
- **NET_ADMIN, NET_RAW, SYS_ADMIN** capabilities for firewall management.
- **Codex CLI auto-install** from the local build artifacts.

### Starting

```bash
devcontainer up --workspace-folder . --config .devcontainer/devcontainer.secure.json
devcontainer exec --workspace-folder . cargo test
```

### Firewall

The `post-start.sh` script runs on every container start:

1. Validates the `OPENAI_ALLOWED_DOMAINS` environment variable.
2. Sets the setuid bit on bubblewrap for inner sandbox support.
3. Installs the Sentinel CLI binary from `target/release` or `target/debug`.
4. Calls `init-firewall.sh` which:
   - Resolves whitelist domains to IPs (via `dig`/`host`/`nslookup`).
   - Loads them into an `ipset` hash set.
   - Sets default-deny OUTPUT policy via iptables.
   - Blocks all IPv6 traffic via ip6tables.
   - Verifies the firewall with curl tests.

### Customising the allowed domains

Set `OPENAI_ALLOWED_DOMAINS` in `devcontainer.secure.json` or as a
container env var:

```json
"containerEnv": {
  "OPENAI_ALLOWED_DOMAINS": "github.com crates.io pypi.org api.openai.com"
}
```

### Building the image

```bash
docker build -f .devcontainer/Dockerfile -t sentinel-dev .
```

---

## Port Mapping

If running the Sentinel app server inside the container, forward port 7860:

```bash
devcontainer exec --workspace-folder . sentinel server start --port 7860
```

The `devcontainer up` command automatically maps ports specified in
`appPort` (or you can use `docker run -p 7860:7860`).

---

## Troubleshooting

### Firewall not applying

Ensure the container has `NET_ADMIN` and `NET_RAW` capabilities:

```json
"capAdd": ["NET_ADMIN", "NET_RAW"]
```

### bubblewrap fails

Verify seccomp is unconfined:

```json
"securityOpt": ["seccomp=unconfined"]
```

Check the setuid bit:

```bash
ls -la /usr/bin/bwrap
# Should show -rwsr-xr-x (s in owner execute position)
```

### Outbound connection failures

Check the firewall status:

```bash
iptables -L -v -n | head -20
ipset list sentinel-whitelist | head -10
```

Temporarily disable for debugging (inside a secure container):

```bash
iptables -P OUTPUT ACCEPT
```
