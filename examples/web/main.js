// Minimal web host for the Kineto runtime: load, play, render, forward taps.
// Served from the repository root (see README.md), so these paths resolve to
// the committed runtime and a committed sample document.
import { loadKinetoRuntime } from "../../runtimes/web/src/index.js";

const DOCUMENT_URL = new URL("../../packages/kineto_runtime/example/assets/sample.kvk", import.meta.url);
const canvas = document.getElementById("kineto-canvas");
const status = document.getElementById("status");

function report(text) {
  status.textContent = text;
  // Exposed for automated smoke checks (examples/web/smoke.mjs).
  globalThis.__kinetoExample = { ...(globalThis.__kinetoExample ?? {}), status: text };
}

async function main() {
  const { KinetoWasmPlayer } = await loadKinetoRuntime();
  const response = await fetch(DOCUMENT_URL);
  if (!response.ok) throw new Error(`fetch ${DOCUMENT_URL}: ${response.status}`);
  const player = new KinetoWasmPlayer(new Uint8Array(await response.arrayBuffer()));

  let frames = 0;
  let last = performance.now();
  function frame(now) {
    player.advance(Math.min((now - last) / 1000, 0.1));
    last = now;
    player.renderToCanvas(canvas.id);
    frames += 1;
    globalThis.__kinetoExample = { ...globalThis.__kinetoExample, frames };
    requestAnimationFrame(frame);
  }
  requestAnimationFrame(frame);

  function forward(kind, event) {
    const rect = canvas.getBoundingClientRect();
    const [x, y] = player.documentPoint(
      event.clientX - rect.left,
      event.clientY - rect.top,
      rect.width,
      rect.height,
    );
    const fired = player.pointerEvent(kind, x, y);
    const target = player.hitTestDocumentPoint(x, y) ?? "nothing";
    report(`${kind} at (${x.toFixed(1)}, ${y.toFixed(1)}): hit ${target}, ${fired} listener(s) fired`);
  }
  canvas.addEventListener("pointerdown", (event) => forward("down", event));
  canvas.addEventListener("pointerup", (event) => forward("up", event));

  report("playing");
}

main().catch((error) => {
  report(`error: ${error.message ?? error}`);
  console.error(error);
});
