import { spawn } from "node:child_process";
import { createServer } from "node:http";
import { createReadStream, statSync } from "node:fs";
import { createRequire } from "node:module";
import { extname, join, resolve } from "node:path";

const root = resolve(new URL("../..", import.meta.url).pathname);
const webRoot = resolve(root, "packages/kineto_runtime/example/build/web");
const port = Number(process.env.KINETO_WEB_SMOKE_PORT || 9087);
const { chromium } = await importPlaywright();

await run("flutter", ["build", "web"], {
  cwd: resolve(root, "packages/kineto_runtime/example"),
});

const server = createStaticServer(webRoot);
await new Promise((resolveListen) => server.listen(port, "127.0.0.1", resolveListen));

try {
  const browser = await chromium.launch({ headless: true });
  const page = await browser.newPage({ viewport: { width: 1200, height: 800 } });
  const messages = [];
  page.on("console", (message) => messages.push(`${message.type()}: ${message.text()}`));
  page.on("pageerror", (error) => messages.push(`pageerror: ${error.message}`));
  await page.goto(`http://127.0.0.1:${port}/`, {
    waitUntil: "networkidle",
    timeout: 30000,
  });
  await page.waitForTimeout(2500);

  const bridgeReady = await page.evaluate(() => Boolean(globalThis.__kinetoRuntimeBridge));
  const wasmFetchOk = await page.evaluate(async () => {
    const response = await fetch("assets/packages/kineto_runtime/web/pkg/kineto_engine_bg.wasm");
    return response.ok && (await response.arrayBuffer()).byteLength > 0;
  });
  const canvasHasContent = await page.evaluate(() => {
    const canvas = document.querySelector("canvas[id^='kineto-runtime-canvas-']");
    if (!canvas) return false;
    const context = canvas.getContext("2d");
    if (!context) return false;
    const pixels = context.getImageData(0, 0, canvas.width, canvas.height).data;
    for (let index = 0; index < pixels.length; index += 4) {
      if (pixels[index] !== 16 || pixels[index + 1] !== 20 || pixels[index + 2] !== 25) {
        return true;
      }
    }
    return false;
  });
  const initialPlaybackState = await readPlaybackState(page);
  await page.waitForTimeout(800);
  const advancedPlaybackState = await readPlaybackState(page);
  await browser.close();

  if (!bridgeReady) throw new Error("Kineto web bridge was not registered");
  if (!wasmFetchOk) throw new Error("Kineto wasm asset did not fetch");
  if (!canvasHasContent) throw new Error("Kineto canvas stayed blank");
  if (
    !(advancedPlaybackState.currentTime > initialPlaybackState.currentTime) ||
    !(advancedPlaybackState.ticks > initialPlaybackState.ticks)
  ) {
    throw new Error(
      `Kineto autoplay did not advance (${JSON.stringify(initialPlaybackState)} -> ${JSON.stringify(advancedPlaybackState)})`,
    );
  }

  const errors = messages.filter((message) => message.startsWith("error:") || message.startsWith("pageerror:"));
  if (errors.length > 0) {
    throw new Error(`Browser errors:\n${errors.join("\n")}`);
  }

  console.log("✓ Flutter web smoke passed");
} finally {
  await new Promise((resolveClose) => server.close(resolveClose));
}

function createStaticServer(baseDir) {
  const contentTypes = {
    ".html": "text/html",
    ".js": "text/javascript",
    ".json": "application/json",
    ".wasm": "application/wasm",
    ".otf": "font/otf",
  };
  return createServer((request, response) => {
    const url = new URL(request.url ?? "/", "http://localhost");
    const pathname = url.pathname === "/" ? "/index.html" : decodeURIComponent(url.pathname);
    const path = join(baseDir, pathname);
    try {
      const stat = statSync(path);
      if (!stat.isFile()) throw new Error("not a file");
      response.writeHead(200, {
        "content-type": contentTypes[extname(path)] ?? "application/octet-stream",
        "content-length": stat.size,
      });
      createReadStream(path).pipe(response);
    } catch {
      response.writeHead(404);
      response.end("Not found");
    }
  });
}

function run(command, args, options) {
  return new Promise((resolveRun, rejectRun) => {
    const child = spawn(command, args, { ...options, stdio: "inherit" });
    child.on("exit", (code) => {
      if (code === 0) {
        resolveRun();
      } else {
        rejectRun(new Error(`${command} ${args.join(" ")} exited ${code}`));
      }
    });
  });
}

async function importPlaywright() {
  try {
    return await import("playwright");
  } catch {
    const require = createRequire(import.meta.url);
    return require("playwright");
  }
}

async function readPlaybackState(page) {
  const value = await page.evaluate(() => {
    return {
      currentTime: Number(globalThis.__kinetoRuntimeCurrentTime),
      ticks: Number(globalThis.__kinetoRuntimeAutoplayTicks),
    };
  });
  if (!Number.isFinite(value.currentTime) || !Number.isFinite(value.ticks)) {
    throw new Error("Kineto playback state was not visible");
  }
  return value;
}
