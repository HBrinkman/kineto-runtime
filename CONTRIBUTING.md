# Contributing

Kineto Runtime is the playback engine and platform SDKs for documents exported
by the Kineto editor. The editor itself is not in this repository.

## Before you open a pull request

```sh
scripts/bootstrap.sh check   # prerequisites, see docs/BUILDING.md
scripts/bootstrap.sh test    # what CI (runtime-tests) runs
```

- Keep runtime behaviour covered by a test: engine tests in
  `engine/kineto_engine/tests`, shared cross-runtime cases in
  `runtimes/parity/runtime_fixtures.json` (and its mirror under
  `packages/kineto_runtime/example/assets/parity/`, which must stay identical).
- After changing `engine/kineto_engine/src/wasm.rs`, rebuild the web package
  (`scripts/bootstrap.sh web`) and commit `runtimes/web/pkg` and its mirror.
- After changing the C ABI, commit the regenerated `kineto_engine.h` and keep
  the symbol lists in the podspec, the plugin `Package.swift` and
  `tool/check_runtime_symbols.sh` in sync.
- Do not commit built native libraries (`.a`, `.so`, `.dylib`).

## Fixtures, fonts, images and sounds

Every binary or media file must be listed in `licenses/media-manifest.tsv`
with its SHA-256 and where it came from. Contribute only fixtures you made
yourself or whose license allows redistribution, and say which in the pull
request. Do not add characters, logos, artwork, fonts or audio from other
brands or products. `tool/check_extraction_boundary.py` enforces the
manifest.

## Dependencies

A new Rust dependency changes `licenses/rust-crates.tsv`; regenerate it with
`tool/rust_license_inventory.py`. A license expression not already on the
allowlist needs review before it is added.

## License

By contributing you agree that your contribution is licensed under the MIT
License in `LICENSE`.
