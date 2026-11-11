#!/usr/bin/env python3
"""Fail if a distributable binary embeds a path from the machine that built it.

Rust bakes source paths into panic locations and file!(). The build scripts
remap the checkout, cargo home and target directory (--remap-path-prefix);
this check proves it worked, so a home directory or user name never ships in
a published library.

Usage: tool/check_binary_paths.py FILE_OR_DIR [...]
Directories are searched for .a, .so, .dylib and .wasm files.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

PATTERN = re.compile(
    rb"/Users/[A-Za-z0-9._-]+/|/home/[a-z_][a-z0-9._-]*/|/Volumes/[^/\0]+/|/private/var/folders/|[A-Za-z]:\\\\Users\\\\"
)
SUFFIXES = {".a", ".so", ".dylib", ".wasm"}

# Toolchain identification strings that name the toolchain vendor's own build
# machine, not ours: the Android NDK's LLD records the path it was built at in
# every linked library's .comment section.
ALLOWED = re.compile(rb"\(/Volumes/Android/buildbot/src/googleplex-android/llvm-r[0-9a-z]+-release/")


def files(args: list[str]) -> list[Path]:
    out: list[Path] = []
    for arg in args:
        path = Path(arg)
        if path.is_dir():
            out.extend(sorted(p for p in path.rglob("*") if p.suffix in SUFFIXES and p.is_file()))
        elif path.is_file():
            out.append(path)
        else:
            raise SystemExit(f"error: no such file: {arg}")
    return out


def main() -> int:
    targets = files(sys.argv[1:])
    if not targets:
        print("error: no binaries to check", file=sys.stderr)
        return 2
    bad = 0
    for path in targets:
        data = ALLOWED.sub(b"(<toolchain>/", path.read_bytes())
        hits = PATTERN.findall(data)
        if hits:
            bad += 1
            # Show the kind of prefix, not the user name.
            kinds = sorted({h.split(b"/")[1].decode(errors="replace") if h.startswith(b"/") else "Users" for h in hits})
            print(f"FAIL: {path}: {len(hits)} host path(s) ({', '.join(kinds)})", file=sys.stderr)
    if bad:
        return 1
    print(f"OK: no host paths in {len(targets)} binar{'y' if len(targets) == 1 else 'ies'}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
