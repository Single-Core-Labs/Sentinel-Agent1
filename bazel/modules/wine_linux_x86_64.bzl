"""External dependency: Wine for Linux x86_64.

Used for cross-platform testing of Windows binaries under Linux CI
without requiring Windows runners.
"""

WINE_LINUX_X86_64 = struct(
    name = "wine_linux_x86_64",
    version = "9.0",
    urls = [
        "https://github.com/wine-mirror/wine/releases/download/wine-9.0/wine-9.0.tar.xz",
    ],
    strip_prefix = "wine-9.0",
    sha256 = "a1f143e75bcb0b5a98e78f88ffb3c2c8b5abf1d4e7b2d7e5e7f1e7e7e7e7e7e7e",  # placeholder
    build_file = "@//third_party/wine:BUILD.wine.bazel",
)
