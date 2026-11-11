# Web example

A static page that loads `packages/kineto_runtime/example/assets/sample.kvk`
with the committed web runtime (`runtimes/web`), plays it and draws it on a
canvas with the Canvas 2D fallback renderer. Taps become `pointerEvent("down")`
/ `("up")` calls; the status line shows the hit target.

No build step and no dependencies. From the repository root:

```sh
node examples/web/serve.mjs        # prints http://127.0.0.1:8787/examples/web/
```

The page imports `../../runtimes/web/src/index.js`, so it must be served from
the repository root, as `serve.mjs` does. To use the runtime in your own site,
copy `runtimes/web/pkg` and `runtimes/web/src` from a known commit.

See `runtimes/web/README.md` for the API and what the web runtime does not
support yet.
