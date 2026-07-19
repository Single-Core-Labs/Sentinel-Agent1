#!/usr/bin/env python3
"""Post-installation script for Sentinel AI dev containers.

Handles:
- Persistent shell history setup
- Directory ownership fixes for bind-mounted workspaces
- Git configuration
"""
import os
import pwd
import shutil
import subprocess
import sys

HOME = os.environ.get("HOME", "/home/vscode")
USER = os.environ.get("USER", "vscode")


def fix_ownership(path: str) -> None:
    """Ensure the given path is owned by the container user."""
    try:
        uid = pwd.getpwnam(USER).pw_uid
        gid = pwd.getpwnam(USER).pw_gid
        os.chown(path, uid, gid)
    except (KeyError, PermissionError, OSError):
        pass


def setup_history() -> None:
    """Configure persistent shell history via mounted volume."""
    history_dir = "/commandhistory"
    if os.path.isdir(history_dir):
        bash_history = os.path.join(history_dir, ".bash_history")
        zsh_history = os.path.join(history_dir, ".zsh_history")

        for rc_file, hist_file, hist_var in [
            (os.path.join(HOME, ".bashrc"), bash_history, "HISTFILE"),
            (os.path.join(HOME, ".zshrc"), zsh_history, "HISTFILE"),
        ]:
            if os.path.isfile(rc_file):
                with open(rc_file, "a") as f:
                    f.write(f"\nexport {hist_var}={hist_file}\n")
                    f.write("export HISTSIZE=100000\n")
                    f.write("export HISTFILESIZE=100000\n")
                    f.write("export HISTCONTROL=ignoreboth:erasedups\n")

        fix_ownership(history_dir)
        print("[post_install] Shell history configured")


def setup_git() -> None:
    """Configure Git for the monorepo."""
    git_config = {
        "core.autocrlf": "input",
        "core.symlinks": "true",
        "core.fsmonitor": "true",
        "pull.rebase": "true",
        "fetch.prune": "true",
        "diff.renameLimit": "9999",
        "safe.directory": "/workspaces/*",
    }

    for key, value in git_config.items():
        subprocess.run(
            ["git", "config", "--global", key, value],
            capture_output=True,
        )

    # Set up Git LFS if available
    if shutil.which("git-lfs"):
        subprocess.run(["git", "lfs", "install", "--skip-repo"], capture_output=True)

    print("[post_install] Git configured")


def main() -> None:
    print("[post_install] Starting…")
    setup_history()
    setup_git()

    # Fix ownership for common workspace paths
    for path in [HOME, os.path.join(HOME, ".cargo"), os.path.join(HOME, ".config")]:
        if os.path.exists(path):
            fix_ownership(path)

    print("[post_install] Done")


if __name__ == "__main__":
    main()
