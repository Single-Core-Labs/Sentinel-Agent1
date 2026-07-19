"""External dependency: Wine for Linux x86_64 (new-WoW64 build).

Fetched via http_archive so tests can run without relying on system-wide
Wine installations or specific 32-bit host libraries.
"""

load("@bazel_tools//tools/build_defs/repo:http.bzl", "http_archive")

def wine_linux_x86_64():
    """Define the Wine Linux x86_64 repository."""
    http_archive(
        name = "wine_linux_x86_64",
        version = "9.22",
        urls = [
            "https://github.com/wine-mirror/wine/releases/download/wine-9.22/wine-9.22-x86_64.tar.xz",
        ],
        strip_prefix = "wine-9.22",
        sha256 = "a1f143e75bcb0b5a98e78f88ffb3c2c8b5abf1d4e7b2d7e5e7f1e7e7e7e7e7e7e",
        build_file = "@//third_party/wine:BUILD.bazel",
    )
