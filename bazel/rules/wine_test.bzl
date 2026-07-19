"""Bazel rules for running Windows executables under Wine during testing.

Provides `wine_test` and `wine_binary` macros that wrap Rust and
native Windows binaries with a Wine execution layer for Linux CI.
"""

load("@rules_rust//rust:defs.bzl", "rust_test", "rust_binary")

def _wine_test_impl(ctx):
    """Run a prebuilt Windows executable under Wine."""
    wine = ctx.executable._wine
    exe = ctx.file.windows_exe
    out = ctx.actions.declare_file(ctx.label.name + ".sh")

    content = """#!/usr/bin/env bash
set -euo pipefail
export WINEDEBUG=-all
export WINEPREFIX=$(mktemp -d)
export WINEARCH=win64
"{wine}" "{exe}" "$@"
ret=$?
rm -rf "$WINEPREFIX"
exit $ret
""".format(wine = wine.path, exe = exe.short_path)

    ctx.actions.write(out, content, is_executable = True)
    return [DefaultInfo(executable = out, runfiles = ctx.runfiles(files = [wine, exe]))]

wine_test = rule(
    implementation = _wine_test_impl,
    attrs = {
        "windows_exe": attr.label(allow_single_file = True, mandatory = True),
        "_wine": attr.label(
            default = "@wine_linux_x86_64//:wine",
            executable = True,
            cfg = "exec",
        ),
    },
    test = True,
)

def wine_rust_test(name, windows_binary, **kwargs):
    """Build a Rust Windows binary and wrap it in a Wine test."""
    binary_name = name + "_windows_bin"
    rust_binary(
        name = binary_name,
        crate_root = windows_binary,
        **kwargs
    )
    wine_test(
        name = name,
        windows_exe = ":" + binary_name,
    )

def windows_cross_test(name, **kwargs):
    """Convenience alias for cross-platform Wine tests."""
    wine_rust_test(name = name, **kwargs)
