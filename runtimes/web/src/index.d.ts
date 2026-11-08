export interface KinetoRuntimeDiagnostics {
  platform: "web";
  renderer: string;
}

export { initSync, KinetoWasmPlayer } from "../pkg/kineto_engine";
export { default as initKinetoRuntime } from "../pkg/kineto_engine";

export function loadKinetoRuntime(input?: unknown): Promise<{
  module: unknown;
  KinetoWasmPlayer: typeof import("../pkg/kineto_engine").KinetoWasmPlayer;
}>;

export function diagnostics(): KinetoRuntimeDiagnostics;
