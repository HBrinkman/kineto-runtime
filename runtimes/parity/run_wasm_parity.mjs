import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import initKinetoRuntime, {
  KinetoWasmPlayer,
} from "../web/pkg/kineto_engine.js";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const manifestPath = resolve(root, "runtimes/parity/runtime_fixtures.json");
const wasmPath = resolve(root, "runtimes/web/pkg/kineto_engine_bg.wasm");
const manifest = JSON.parse(readFileSync(manifestPath, "utf8"));

await initKinetoRuntime({ module_or_path: readFileSync(wasmPath) });

let executed = 0;
for (const fixture of manifest.fixtures) {
  if (fixture.platform && fixture.platform !== "web") continue;
  runFixture(fixture);
  executed += 1;
}

console.log(`✓ wasm parity fixtures passed (${executed})`);

function runFixture(fixture) {
  let player = null;
  for (const step of fixture.steps) {
    if (step.op === "load") {
      player = new KinetoWasmPlayer(readFileSync(resolve(root, fixture.asset)));
      continue;
    }
    if (!player) throw new Error(`${fixture.id}: player not loaded`);

    switch (step.op) {
      case "advance":
        player.advance(step.deltaSeconds);
        break;
      case "pointerEvent": {
        const fired = player.pointerEvent(step.event, step.x, step.y);
        if (step.expectFired !== undefined && fired !== step.expectFired) {
          throw new Error(
            `${fixture.id}: expected ${step.expectFired} listeners to fire, got ${fired}`,
          );
        }
        break;
      }
      case "setNumber":
        player.setNumber(step.stateMachineId, step.inputName, step.value);
        break;
      case "setTrigger":
        player.setTrigger(step.stateMachineId, step.inputName);
        break;
      case "expectActiveState": {
        const actual = player.activeState(step.stateMachineId) ?? null;
        if (actual !== step.stateId) {
          throw new Error(
            `${fixture.id}: expected ${step.stateMachineId} in ${step.stateId}, got ${actual}`,
          );
        }
        break;
      }
      case "expectOverrideScalar": {
        const actual = player.overrideScalar(step.nodeId, step.key) ?? null;
        const expected = step.value;
        if (actual === null || Math.abs(actual - expected) > 1e-6) {
          throw new Error(
            `${fixture.id}: expected ${step.nodeId}.${step.key} = ${expected}, got ${actual}`,
          );
        }
        break;
      }
      case "expectBackground": {
        const actual = Array.from(player.backgroundColor());
        if (actual.join(",") !== step.rgba.join(",")) {
          throw new Error(
            `${fixture.id}: expected background ${step.rgba}, got ${actual}`,
          );
        }
        break;
      }
      case "expectDocumentPoint": {
        const point = player.documentPoint(
          step.x,
          step.y,
          step.surfaceWidth,
          step.surfaceHeight,
        );
        if (
          point.length !== 2 ||
          Math.abs(point[0] - step.expect[0]) > 1e-6 ||
          Math.abs(point[1] - step.expect[1]) > 1e-6
        ) {
          throw new Error(
            `${fixture.id}: expected documentPoint ${step.expect}, got ${Array.from(point)}`,
          );
        }
        break;
      }
      case "expectAudioEvents": {
        const actual = player
          .audioEvents()
          .map((event) => `${event.id}|${event.name}|${event.sourceName}`);
        expectEqual(fixture, "audioEvents", actual, step.expect);
        break;
      }
      case "expectTakeFiredAudioEvents": {
        // Draining: callers assert a second take is empty with a second step.
        expectEqual(
          fixture,
          "takeFiredAudioEvents",
          Array.from(player.takeFiredAudioEvents()),
          step.expect,
        );
        break;
      }
      case "hitTestDocumentPoint": {
        const nodeId = player.hitTestDocumentPoint(step.x, step.y) ?? null;
        if (nodeId !== fixture.expect.nodeId) {
          throw new Error(`${fixture.id}: expected node ${fixture.expect.nodeId}, got ${nodeId}`);
        }
        break;
      }
      default:
        throw new Error(`${fixture.id}: unsupported op ${step.op}`);
    }
  }
  player?.free();
}

function expectEqual(fixture, what, actual, expected) {
  if (JSON.stringify(actual) !== JSON.stringify(expected)) {
    throw new Error(
      `${fixture.id}: expected ${what} ${JSON.stringify(expected)}, got ${JSON.stringify(actual)}`,
    );
  }
}
