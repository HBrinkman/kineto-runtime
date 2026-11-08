#!/usr/bin/env python3
"""Tests for tool/check_extraction_boundary.py.

Runs the guard against this repository (must be clean) and against throwaway
copies seeded with each kind of content that must never be published.

  python3 -I tool/test_extraction_boundary.py
"""

from __future__ import annotations

import hashlib
import importlib.util
import shutil
import sys
import tempfile
import unittest
from pathlib import Path

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parent.parent
_spec = importlib.util.spec_from_file_location("boundary", ROOT / "tool" / "check_extraction_boundary.py")
boundary = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(boundary)

# Strings assembled at runtime so this file does not itself trip other scanners.
BRAND = "Duo" + "lingo"
PRIVATE_REPO = "https://github.com/" + "HBrinkman/kineto" + ".git"
TEAM = "DEVELOPMENT_TEAM = " + "ABCDE12345" + ";"
KEY = "-----BEGIN " + "PRIVATE KEY-----"


class Tree:
    """A minimal copy of the files the guard reads, outside git."""

    def __init__(self) -> None:
        self.dir = Path(tempfile.mkdtemp(prefix="kineto-boundary-"))
        for rel in boundary.listed_files(ROOT):
            src = ROOT / rel
            dst = self.dir / rel
            dst.parent.mkdir(parents=True, exist_ok=True)
            if src.is_symlink():
                dst.symlink_to(src.readlink())
            else:
                shutil.copyfile(src, dst)

    def write(self, rel: str, data: bytes | str) -> None:
        path = self.dir / rel
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(data.encode() if isinstance(data, str) else data)

    def problems(self) -> list[str]:
        return boundary.check(self.dir)

    def close(self) -> None:
        shutil.rmtree(self.dir)


class BoundaryTest(unittest.TestCase):
    def setUp(self) -> None:
        self.tree = Tree()

    def tearDown(self) -> None:
        self.tree.close()

    def assertFlags(self, needle: str) -> None:
        problems = self.tree.problems()
        self.assertTrue(any(needle in p for p in problems), f"expected '{needle}' in {problems}")

    def test_repository_is_clean(self) -> None:
        self.assertEqual(boundary.check(ROOT), [])

    def test_copy_is_clean(self) -> None:
        self.assertEqual(self.tree.problems(), [])

    def test_editor_source_is_denied(self) -> None:
        self.tree.write("lib/src/app.dart", "void main() {}\n")
        self.assertFlags("denied path: lib/src/app.dart")

    def test_editor_schema_is_denied(self) -> None:
        self.tree.write("schema/scene_delta.fbs", "table SceneDelta {}\n")
        self.assertFlags("denied path: schema/scene_delta.fbs")

    def test_agent_config_and_internal_docs_are_denied(self) -> None:
        self.tree.write(".claude/settings.json", "{}")
        self.tree.write("CLAUDE.md", "notes")
        self.tree.write("docs/research/notes.md", "notes")
        self.assertFlags("denied path: .claude/settings.json")
        self.assertFlags("denied path: CLAUDE.md")
        self.assertFlags("denied path: docs/research/notes.md")

    def test_unknown_top_level_path_is_rejected(self) -> None:
        self.tree.write("notes/todo.md", "x")
        self.assertFlags("path outside the runtime allowlist: notes/todo.md")

    def test_signing_material_is_denied(self) -> None:
        for rel in [
            "runtimes/android/release.keystore",
            "packages/kineto_runtime/example/android/key.properties",
            "packages/kineto_runtime/example/ios/dist.p12",
            "runtimes/swift/KinetoRuntime/AuthKey_ABC.p8",
            "packages/kineto_runtime/example/ios/Runner/GoogleService-Info.plist",
        ]:
            self.tree.write(rel, "secret")
            self.assertFlags(f"denied path: {rel}")

    def test_generated_native_binaries_are_denied(self) -> None:
        rel = "runtimes/swift/KinetoRuntime/Artifacts/KinetoRuntimeC.xcframework/ios-arm64/libkineto_engine.a"
        self.tree.write(rel, b"!<arch>\n")
        self.assertFlags(f"denied path: {rel}")
        rel = "runtimes/android/KinetoRuntime/src/main/jniLibs/arm64-v8a/libkineto_engine.so"
        self.tree.write(rel, b"\x7fELF")
        self.assertFlags(f"denied path: {rel}")

    def test_unreviewed_fixture_is_rejected(self) -> None:
        self.tree.write("engine/kineto_engine/tests/fixtures/character.kvk", b"KVKD\0\0")
        self.assertFlags("unreviewed media/binary")

    def test_unreviewed_image_and_audio_are_rejected(self) -> None:
        self.tree.write("packages/kineto_runtime/example/assets/mascot.png", b"\x89PNG\r\n")
        self.tree.write("packages/kineto_runtime/example/assets/jingle.wav", b"RIFF\0\0")
        problems = self.tree.problems()
        self.assertEqual(sum("unreviewed media/binary" in p for p in problems), 2, problems)

    def test_replaced_fixture_bytes_are_rejected(self) -> None:
        self.tree.write("packages/kineto_runtime/example/assets/sample.kvk", b"different bytes")
        self.assertFlags("media hash changed since review")

    def test_brand_name_in_text_is_rejected(self) -> None:
        self.tree.write("engine/kineto_engine/tests/character.rs", f"// {BRAND} character\n")
        self.assertFlags("third-party brand in engine/kineto_engine/tests/character.rs:1")

    def test_brand_fixture_table_entry_is_rejected(self) -> None:
        rel = "runtimes/parity/runtime_fixtures.json"
        text = (ROOT / rel).read_text().replace('"sample_load_advance"', '"sm-duo"', 1)
        self.tree.write(rel, text)
        self.assertFlags("third-party brand in runtimes/parity/runtime_fixtures.json")

    def test_private_references_are_rejected(self) -> None:
        self.tree.write("README.md", f"Clone {PRIVATE_REPO}\nSee /Users/someone/kineto\n")
        self.assertFlags("private editor repo URL in README.md:1")
        self.assertFlags("local user path in README.md:2")

    def test_public_repo_url_is_allowed(self) -> None:
        self.tree.write("README.md", "https://github.com/HBrinkman/kineto-runtime\n")
        self.assertEqual(self.tree.problems(), [])

    def test_team_id_and_keys_are_rejected(self) -> None:
        self.tree.write("packages/kineto_runtime/example/ios/Runner.xcodeproj/project.pbxproj", TEAM + "\n")
        self.tree.write("runtimes/web/src/key.js", KEY + "\n")
        self.assertFlags("Apple team ID")
        self.assertFlags("private key in runtimes/web/src/key.js")

    def test_fixture_table_with_missing_asset_is_rejected(self) -> None:
        (self.tree.dir / "packages/kineto_runtime/example/assets/pointer_binding.kvk").unlink()
        self.assertFlags("names missing asset packages/kineto_runtime/example/assets/pointer_binding.kvk")

    def test_fixture_table_mirror_drift_is_rejected(self) -> None:
        rel = "packages/kineto_runtime/example/assets/parity/runtime_fixtures.json"
        self.tree.write(rel, (ROOT / rel).read_text() + "\n")
        self.assertFlags("the two runtime_fixtures.json copies differ")

    def test_escaping_symlink_is_rejected(self) -> None:
        link = self.tree.dir / "runtimes/web/src/outside"
        link.symlink_to("../../../../../etc")
        self.assertFlags("symlink leaves the repository")

    def test_manifest_hashes_match_files(self) -> None:
        for rel, digest in boundary.load_manifest(ROOT).items():
            self.assertEqual(hashlib.sha256((ROOT / rel).read_bytes()).hexdigest(), digest, rel)


class GlobTest(unittest.TestCase):
    def test_patterns(self) -> None:
        m = boundary.matches
        self.assertTrue(m("lib/a.dart", ["lib/**"]))
        self.assertFalse(m("packages/kineto_runtime/lib/a.dart", ["lib/**"]))
        self.assertTrue(m("a/b/key.properties", ["**/key.properties"]))
        self.assertTrue(m("key.properties", ["**/key.properties"]))
        self.assertTrue(m("docs/design-research/x.md", ["docs/design*/**"]))
        self.assertFalse(m("docs/BUILDING.md", ["docs/design*/**"]))
        self.assertTrue(m("x/y/libz.a", ["**/*.a"]))
        self.assertFalse(m("x/y/data.json", ["**/*.a"]))


if __name__ == "__main__":
    unittest.main(verbosity=1)
