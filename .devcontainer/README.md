# Development Containers

## Standard Profile

The default `devcontainer.json` provides a full Rust + Python + Node.js
development environment with Bazel, Docker-in-Docker, and all Sentinel
dependencies pre-installed.

## Secure / Network-Restricted Profile

The `secure` profile (`devcontainer.secure.json`) adds:

- **Firewall**: iptables + ipset rules blocking all outbound traffic except
  to whitelisted domains (GitHub, crates.io, pypi.org, npmjs.org, etc.).
- **IPv6 blocking**: all IPv6 traffic is dropped.
- **Bubblewrap sandbox**: processes spawned during tests or agent execution
  run inside a bwrap sandbox with minimal capabilities, no network, and a
  read-only filesystem overlay.

### Testing with the secure profile

```bash
# Build and start
devcontainer up --workspace-folder . --config .devcontainer/devcontainer.secure.json

# Run the full test suite inside the sandbox
devcontainer exec --workspace-folder . cargo test
```

## Post-Install

The `post_install.py` script runs on container creation:
- Sets up persistent shell history (mounted volume)
- Fixes directory ownership for bind-mounted workspaces
- Configures Git with sensible defaults for the monorepo
