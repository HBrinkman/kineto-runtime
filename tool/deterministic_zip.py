#!/usr/bin/env python3
"""Write a byte-for-byte reproducible zip of one or more directory entries.

The same input tree always produces the same archive: entries are sorted,
every timestamp is 1980-01-01 00:00:00 (the zip epoch), permissions are
normalised to 0644 for files and 0755 for directories and executables, no
extra fields (uid/gid, extended timestamps) are written, and compression is
deflate at a fixed level. Symlinks are rejected: an XCFramework or a pub
package archive must not depend on links that may not survive extraction.

Usage:
  tool/deterministic_zip.py --base DIR --output OUT.zip ENTRY [ENTRY ...]

Each ENTRY is a path relative to --base (a file or a directory, recursed).
"""

from __future__ import annotations

import argparse
import os
import stat
import sys
import zipfile
from pathlib import Path

EPOCH = (1980, 1, 1, 0, 0, 0)


def collect(base: Path, entries: list[str]) -> list[Path]:
    paths: set[Path] = set()
    for entry in entries:
        root = base / entry
        if root.is_symlink():
            raise SystemExit(f"error: symlink not allowed in archive: {entry}")
        if not root.exists():
            raise SystemExit(f"error: missing entry: {entry}")
        paths.add(root)
        if root.is_dir():
            for dirpath, dirnames, filenames in os.walk(root, followlinks=False):
                dirnames.sort()
                for name in dirnames + filenames:
                    child = Path(dirpath) / name
                    if child.is_symlink():
                        raise SystemExit(f"error: symlink not allowed in archive: {child.relative_to(base)}")
                    if name == ".DS_Store":
                        continue
                    paths.add(child)
    # Parent directories are implied by every member path; add them too so
    # extraction recreates empty directories identically.
    for path in list(paths):
        for parent in path.relative_to(base).parents:
            if str(parent) != ".":
                paths.add(base / parent)
    return sorted(paths, key=lambda p: p.relative_to(base).as_posix())


def write(base: Path, output: Path, paths: list[Path]) -> None:
    output.parent.mkdir(parents=True, exist_ok=True)
    tmp = output.with_suffix(output.suffix + ".tmp")
    with zipfile.ZipFile(tmp, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
        for path in paths:
            rel = path.relative_to(base).as_posix()
            if path.is_dir():
                info = zipfile.ZipInfo(rel + "/", date_time=EPOCH)
                info.external_attr = (stat.S_IFDIR | 0o755) << 16 | 0x10
                info.create_system = 3
                archive.writestr(info, b"")
                continue
            mode = 0o755 if os.access(path, os.X_OK) else 0o644
            info = zipfile.ZipInfo(rel, date_time=EPOCH)
            info.external_attr = (stat.S_IFREG | mode) << 16
            info.create_system = 3
            info.compress_type = zipfile.ZIP_DEFLATED
            archive.writestr(info, path.read_bytes(), compress_type=zipfile.ZIP_DEFLATED, compresslevel=9)
    tmp.replace(output)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--base", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("entries", nargs="+")
    args = parser.parse_args()
    base = args.base.resolve()
    write(base, args.output.resolve(), collect(base, args.entries))
    print(args.output)
    return 0


if __name__ == "__main__":
    sys.exit(main())
