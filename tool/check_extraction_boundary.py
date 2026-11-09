#!/usr/bin/env python3
"""Extraction boundary guard for the public Kineto runtime repository.

This repository is a runtime-only extraction from a private editor repository.
The guard fails when content that must stay private, or that has no
demonstrated redistribution right, enters the tree:

  1. paths outside the runtime allowlist (editor UI, internal docs, agent
     config, the editor-only scene_delta schema, the tessellator, ...);
  2. signing material, keys, account config and generated native binaries;
  3. media and binary files (fixtures, fonts, images, audio, jars, wasm) that
     are not listed, with their exact SHA-256, in licenses/media-manifest.tsv;
  4. third-party brand names known to have been in the private fixtures,
     private repository URLs, local user paths, session links, Apple team IDs
     and common credential formats in any text file;
  5. parity fixture tables that reference a missing asset or have drifted
     from their mirrored copy.

Usage:
  tool/check_extraction_boundary.py            # check files git would publish
  tool/check_extraction_boundary.py --root DIR # check another tree (tests)

Files are those `git ls-files --cached --others --exclude-standard` reports,
i.e. what is tracked plus what a plain `git add -A` would add.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import subprocess
import sys
from pathlib import Path

# Top-level layout the extraction may contain. Anything else is a violation.
ALLOWED_PATHS = [
    ".github/workflows/runtime-tests.yml",
    ".gitignore",
    "CONTRIBUTING.md",
    "LICENSE",
    "PROVENANCE.md",
    "README.md",
    "SECURITY.md",
    "THIRD_PARTY_NOTICES.md",
    "docs/BUILDING.md",
    "docs/PLATFORM_SUPPORT.md",
    "engine/.cargo/config.toml",
    "engine/Cargo.lock",
    "engine/Cargo.toml",
    "engine/kineto_engine/**",
    "licenses/**",
    "packages/kineto_runtime/**",
    "runtimes/android/**",
    "runtimes/parity/**",
    "runtimes/swift/KinetoRuntime/**",
    "runtimes/web/**",
    "schema/kineto_runtime.fbs",
    "scripts/bootstrap.sh",
    "scripts/build_runtime_android.sh",
    "scripts/build_runtime_web.sh",
    "scripts/runtime_android_flutter_smoke.sh",
    # Shared parity tables the engine tests read; text, generated upstream.
    "test/fixtures/background/background_parity.txt",
    "test/fixtures/blend/blend_cases.txt",
    "test/fixtures/camera/camera_parity.txt",
    "test/fixtures/constraints/rig_parity.txt",
    "test/fixtures/constraints/shape_constraints.txt",
    "test/fixtures/geometry/path_segments.txt",
    "test/fixtures/layers/body_face_mouth_poses.txt",
    "test/fixtures/pose/pose_runtime_frames.txt",
    "test/fixtures/stroke_trim_windows.txt",
    "tool/check_extraction_boundary.py",
    "tool/check_runtime_symbols.sh",
    "tool/ensure_ios_runtime_artifacts.sh",
    "tool/refresh_runtime_xcframework.sh",
    "tool/rust_license_inventory.py",
    "tool/test_extraction_boundary.py",
]

# Explicitly forbidden even if a future allowlist entry is too broad.
DENIED_PATHS = [
    "lib/**",
    "ios/**",
    "macos/**",
    "assets/**",
    "artifacts/**",
    "integration_test/**",
    "docs/research/**",
    "docs/design*/**",
    "docs/marketing/**",
    ".claude/**",
    ".codex/**",
    ".githooks/**",
    "AGENTS.md",
    "CLAUDE.md",
    "GEMINI.md",
    "schema/scene_delta.fbs",
    "engine/kineto_tessellator/**",
    "runtimes/swift/KinetoSoundsReference/**",
    "config/**",
    "**/*.p12",
    "**/*.p8",
    "**/*.cer",
    "**/*.mobileprovision",
    "**/*.provisionprofile",
    "**/*.keystore",
    "**/*.jks",
    "**/key.properties",
    "**/local.properties",
    "**/.env",
    "**/.env.*",
    "**/GoogleService-Info.plist",
    "**/google-services.json",
    "**/ExportOptions*.plist",
    "**/*.a",
    "**/*.so",
    "**/*.dylib",
    "**/*.dll",
    "**/*.xcarchive/**",
    "**/*.ipa",
    "**/*.apk",
    "**/*.aab",
    "**/xcuserdata/**",
]

# Extensions treated as media/binary: each file must be in the media manifest
# with its exact hash, so a new image, font, sound or fixture is a reviewed act.
MEDIA_EXTENSIONS = {
    ".kvk", ".kvkproj", ".ttf", ".otf", ".woff", ".woff2", ".png", ".jpg",
    ".jpeg", ".gif", ".webp", ".heic", ".svg", ".wav", ".mp3", ".m4a", ".aac",
    ".caf", ".ogg", ".mp4", ".mov", ".pdf", ".jar", ".wasm", ".zip",
}

# Case-insensitive terms that must not appear in any published text file. The
# brand terms name third-party characters/brands that were in private fixtures.
FORBIDDEN_TERMS = [
    ("third-party brand", re.compile(r"duolingo|\bduo(con|lingo)?\b|sm-duo", re.I)),
    ("private editor repo URL", re.compile(r"github\.com[:/]+HBrinkman/kineto(?![-\w])", re.I)),
    ("local user path", re.compile(r"/Users/[A-Za-z0-9._-]+/|/home/[a-z][a-z0-9._-]+/|C:\\\\Users\\\\", re.I)),
    ("agent/session link", re.compile(r"claude\.ai/(chat|code|share)|chatgpt\.com/(c|share)/|\.hermes/", re.I)),
    ("Apple team ID", re.compile(r"DEVELOPMENT_TEAM\s*=\s*\"?[A-Z0-9]{10}")),
    ("signing identity", re.compile(r"(Apple|iPhone) (Distribution|Development): ")),
    ("provisioning profile", re.compile(r"PROVISIONING_PROFILE(_SPECIFIER)?\s*=\s*\"?[A-Za-z0-9-]{8,}")),
    ("private key", re.compile(r"-----BEGIN [A-Z ]*PRIVATE KEY-----")),
    ("AWS key", re.compile(r"\bAKIA[0-9A-Z]{16}\b")),
    ("GitHub token", re.compile(r"\bgh[pousr]_[A-Za-z0-9]{36,}\b|github_pat_[A-Za-z0-9_]{40,}")),
    ("Slack token", re.compile(r"\bxox[abprs]-[A-Za-z0-9-]{10,}")),
    ("API secret key", re.compile(r"\bsk-(ant-|live_|proj-)?[A-Za-z0-9_-]{20,}")),
    ("Google API key", re.compile(r"\bAIza[0-9A-Za-z_-]{35}\b")),
    ("private keystore password", re.compile(r"(storePassword|keyPassword)\s*[=:]\s*\S+", re.I)),
]

# Files allowed to spell out the forbidden patterns, because they define them.
TERM_EXEMPT = {
    "tool/check_extraction_boundary.py",
    "tool/test_extraction_boundary.py",
}

FIXTURE_TABLES = [
    "runtimes/parity/runtime_fixtures.json",
    "packages/kineto_runtime/example/assets/parity/runtime_fixtures.json",
]

MEDIA_MANIFEST = "licenses/media-manifest.tsv"


def glob_regex(pattern: str) -> re.Pattern[str]:
    """`**/` = any leading directories (or none), `/**` = anything below,
    `*` and `?` stay inside one path segment."""
    out = ""
    i = 0
    while i < len(pattern):
        if pattern.startswith("**/", i):
            out += "(?:.*/)?"
            i += 3
        elif pattern.startswith("/**", i) and i + 3 == len(pattern):
            out += "/.*"
            i += 3
        elif pattern[i] == "*":
            out += "[^/]*"
            i += 1
        elif pattern[i] == "?":
            out += "[^/]"
            i += 1
        else:
            out += re.escape(pattern[i])
            i += 1
    return re.compile(out + r"\Z")


def matches(path: str, patterns: list[str]) -> bool:
    return any(glob_regex(pattern).match(path) for pattern in patterns)


def listed_files(root: Path) -> list[str]:
    try:
        out = subprocess.run(
            ["git", "-C", str(root), "ls-files", "-z", "--cached", "--others", "--exclude-standard"],
            check=True,
            capture_output=True,
        ).stdout
        files = [p for p in out.decode().split("\0") if p]
        # A deletion not yet committed still shows as cached; skip what is gone.
        return sorted(p for p in files if (root / p).exists() or (root / p).is_symlink())
    except (subprocess.CalledProcessError, FileNotFoundError):
        return sorted(
            str(p.relative_to(root)).replace("\\", "/")
            for p in root.rglob("*")
            if (p.is_file() or p.is_symlink()) and ".git" not in p.relative_to(root).parts
        )


def load_manifest(root: Path) -> dict[str, str]:
    manifest = {}
    path = root / MEDIA_MANIFEST
    if not path.is_file():
        return manifest
    for line in path.read_text().splitlines():
        if not line or line.startswith("#") or line.startswith("path\t"):
            continue
        fields = line.split("\t")
        manifest[fields[0]] = fields[1]
    return manifest


def is_text(data: bytes) -> bool:
    if b"\0" in data[:8192]:
        return False
    try:
        data.decode("utf-8")
        return True
    except UnicodeDecodeError:
        return False


def check(root: Path) -> list[str]:
    problems: list[str] = []
    files = listed_files(root)
    manifest = load_manifest(root)
    seen_media = set()

    for rel in files:
        full = root / rel
        if matches(rel, DENIED_PATHS):
            problems.append(f"denied path: {rel}")
            continue
        if not matches(rel, ALLOWED_PATHS):
            problems.append(f"path outside the runtime allowlist: {rel}")
            continue

        if full.is_symlink():
            target = (full.parent / full.readlink()).resolve()
            try:
                target.relative_to(root.resolve())
            except ValueError:
                problems.append(f"symlink leaves the repository: {rel} -> {full.readlink()}")
            continue

        data = full.read_bytes()
        suffix = full.suffix.lower()
        if suffix in MEDIA_EXTENSIONS or not is_text(data):
            seen_media.add(rel)
            digest = hashlib.sha256(data).hexdigest()
            expected = manifest.get(rel)
            if expected is None:
                problems.append(f"unreviewed media/binary (add to {MEDIA_MANIFEST} with provenance): {rel}")
            elif expected != digest:
                problems.append(f"media hash changed since review: {rel} (manifest {expected[:12]}, file {digest[:12]})")
            if suffix != ".svg":
                continue

        if rel in TERM_EXEMPT:
            continue
        text = data.decode("utf-8", errors="replace")
        for label, pattern in FORBIDDEN_TERMS:
            hit = pattern.search(text)
            if hit:
                line = text.count("\n", 0, hit.start()) + 1
                problems.append(f"{label} in {rel}:{line}")

    for rel in sorted(set(manifest) - seen_media):
        problems.append(f"{MEDIA_MANIFEST} lists a file that is not published: {rel}")

    tables = {}
    for rel in FIXTURE_TABLES:
        path = root / rel
        if not path.is_file():
            problems.append(f"missing fixture table: {rel}")
            continue
        tables[rel] = path.read_bytes()
        for fixture in json.loads(tables[rel])["fixtures"]:
            asset = fixture["asset"]
            if not (root / asset).is_file():
                problems.append(f"{rel}: fixture {fixture['id']} names missing asset {asset}")
    if len(set(tables.values())) > 1:
        problems.append("the two runtime_fixtures.json copies differ")

    return problems


def main() -> int:
    parser = argparse.ArgumentParser(description="Check the public extraction boundary.")
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parent.parent)
    args = parser.parse_args()
    problems = check(args.root.resolve())
    if problems:
        print(f"FAIL: {len(problems)} extraction boundary problem(s):")
        for problem in problems:
            print(f"  {problem}")
        return 1
    print(f"OK: extraction boundary clean ({len(listed_files(args.root.resolve()))} files)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
