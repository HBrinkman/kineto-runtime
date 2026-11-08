# Kineto Web Runtime

Status: wasm-bindgen package, canvas 2D rendering. Not published to npm
(`"private": true`); partners get it from this repo.

## Source of truth

- **Source:** `engine/kineto_engine/src/wasm.rs`, behind the `web-runtime`
  cargo feature.
- **Build:** `scripts/build_runtime_web.sh` (needs `cargo`, the
  `wasm32-unknown-unknown` target and `wasm-bindgen-cli` pinned to the
  `wasm-bindgen` version in `engine/Cargo.lock`, currently 0.2.122).
- **Output:** `runtimes/web/pkg/` is committed, generated wasm-bindgen
  `--target web` output. Never hand-edit it; change the Rust and rebuild.
  The script mirrors the same files to `packages/kineto_runtime/web/pkg/`
  (the Flutter web package).
- **Distribution:** hand partners `runtimes/web/pkg` (plus `src/` for the thin
  loader) from a tagged commit. Any other build of the runtime, for example a
  website's own copy, is a fork and can drift from this one: rebuild it from
  the same commit with the same script.
- **Check it is current:** after changing `wasm.rs`, rebuild and commit `pkg/`.
  `node ../parity/run_wasm_parity.mjs` (from this folder) loads the committed
  wasm and runs the shared fixtures.

```bash
scripts/build_runtime_web.sh        # from the repo root
cd runtimes/web && npm run test:parity
```

## API (`KinetoWasmPlayer`)

`advance`, `setBool`, `setNumber`, `setTrigger`, `pointerEvent`,
`documentPoint`, `hitTestDocumentPoint`, `activeState`, `activeTransition`,
`transitionProgress`, `overrideScalar`, `audioEvents`, `takeFiredAudioEvents`,
`renderToCanvas`, `lastError`. See `pkg/kineto_engine.d.ts` for signatures.

Audio: a `.kvk` carries no audio bytes. `audioEvents()` lists the document's
sounds; poll `takeFiredAudioEvents()` after `advance` / `pointerEvent` and play
whatever your page ships for each id.

## Not in the web API yet

The native runtimes have these; web does not, and nothing here promises them:
`pointerMoved`, listing state machines and inputs, `selectAnimation`, `pause`.
