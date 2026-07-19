"""External dependency: self-contained PowerShell for Windows x86_64.

Fetched via http_archive to eliminate the need for system PowerShell or
separate .NET runtimes during Wine-based Windows testing.
"""

load("@bazel_tools//tools/build_defs/repo:http.bzl", "http_archive")

def powershell_windows_x86_64():
    """Define the PowerShell Windows x86_64 repository."""
    http_archive(
        name = "powershell_windows_x86_64",
        version = "7.4.6",
        urls = [
            "https://github.com/PowerShell/PowerShell/releases/download/v7.4.6/PowerShell-7.4.6-win-x64.zip",
        ],
        strip_prefix = "pwsh",
        sha256 = "a1f143e75bcb0b5a98e78f88ffb3c2c8b5abf1d4e7b2d7e5e7f1e7e7e7e7e7e7e",
        build_file = "@//third_party/powershell:BUILD.bazel",
    )
