"""External dependency: PowerShell for Windows x86_64.

Used for running PowerShell-based test scripts on Windows hosts.
Fetched via http_archive to avoid relying on system-wide installations.
"""

POWERSHELL_WINDOWS_X86_64 = struct(
    name = "powershell_windows_x86_64",
    version = "7.4.6",
    urls = [
        "https://github.com/PowerShell/PowerShell/releases/download/v7.4.6/PowerShell-7.4.6-win-x64.zip",
    ],
    strip_prefix = "pwsh",
    sha256 = "a1f143e75bcb0b5a98e78f88ffb3c2c8b5abf1d4e7b2d7e5e7f1e7e7e7e7e7e7e",  # placeholder
    build_file = "@//third_party/powershell:BUILD.powershell.bazel",
)
