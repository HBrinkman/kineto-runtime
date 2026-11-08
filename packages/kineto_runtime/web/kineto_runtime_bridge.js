let modulePromise = null;

async function ensureModule() {
  if (!modulePromise) {
    modulePromise = import("./pkg/kineto_engine.js").then(async (module) => {
      await module.default();
      return module;
    });
  }
  return modulePromise;
}

globalThis.__kinetoRuntimeBridge = {
  async createPlayer(bytes) {
    const module = await ensureModule();
    return new module.KinetoWasmPlayer(bytes);
  },
  advance(player, deltaSeconds) {
    player.advance(deltaSeconds);
  },
  setBool(player, stateMachineId, inputName, value) {
    player.setBool(stateMachineId, inputName, value);
  },
  setTrigger(player, stateMachineId, inputName) {
    player.setTrigger(stateMachineId, inputName);
  },
  activeState(player, stateMachineId) {
    return player.activeState(stateMachineId) ?? null;
  },
  activeTransition(player, stateMachineId) {
    return player.activeTransition(stateMachineId) ?? null;
  },
  transitionProgress(player, stateMachineId) {
    return player.transitionProgress(stateMachineId);
  },
  hitTestDocumentPoint(player, x, y) {
    return player.hitTestDocumentPoint(x, y) ?? null;
  },
  renderToCanvas(player, canvasId) {
    player.renderToCanvas(canvasId);
  },
  dispose(player) {
    player.free();
  },
};

await ensureModule();
