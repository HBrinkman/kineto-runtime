import initKinetoRuntime, {
  initSync,
  KinetoWasmPlayer,
} from "../pkg/kineto_engine.js";

export { initKinetoRuntime, initSync, KinetoWasmPlayer };

export async function loadKinetoRuntime(input) {
  const module = await initKinetoRuntime(input);
  return { module, KinetoWasmPlayer };
}

export function diagnostics() {
  return {
    platform: "web",
    renderer: "Web wasm-bindgen runtime ready; WebGPU canvas renderer pending",
  };
}
