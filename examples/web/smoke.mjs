// Headless-browser smoke test for the web example: serves the repository root,
// opens examples/web/ in Chromium, and fails unless the runtime reports
// "playing", frames advance, the canvas has drawn pixels and the console is
// free of errors.
//
//   node examples/web/smoke.mjs     (needs the `playwright` package + Chromium)
import { spawn } from "node:child_process";
import { createRequire } from "node:module";

const port = Number(process.env.KINETO_WEB_EXAMPLE_PORT ?? 8788);
const { chromium } = await importPlaywright();

const server = spawn(process.execPath, [new URL("./serve.mjs", import.meta.url).pathname, String(port)], {
  stdio: ["ignore", "pipe", "inherit"],
});
await new Promise((resolveReady, rejectReady) => {
  server.stdout.on("data", (chunk) => chunk.toString().includes("http://") && resolveReady());
  server.on("exit", (code) => rejectReady(new Error(`server exited ${code}`)));
});

let browser;
try {
  browser = await chromium.launch({ headless: true });
  const page = await browser.newPage({ viewport: { width: 900, height: 900 } });
  const errors = [];
  page.on("console", (message) => message.type() === "error" && errors.push(message.text()));
  page.on("pageerror", (error) => errors.push(error.message));
  await page.goto(`http://127.0.0.1:${port}/examples/web/`, { waitUntil: "networkidle", timeout: 30000 });
  await page.waitForFunction(() => globalThis.__kinetoExample?.status === "playing" && globalThis.__kinetoExample.frames > 10, null, {
    timeout: 15000,
  });
  const drawn = await page.evaluate(() => {
    const canvas = document.getElementById("kineto-canvas");
    const pixels = canvas.getContext("2d").getImageData(0, 0, canvas.width, canvas.height).data;
    const first = pixels.slice(0, 4).join();
    for (let i = 4; i < pixels.length; i += 4) {
      if (pixels.slice(i, i + 4).join() !== first) return true;
    }
    return false;
  });
  if (!drawn) throw new Error("canvas is a single colour: nothing was drawn");
  if (errors.length) throw new Error(`browser errors:\n${errors.join("\n")}`);
  console.log("✓ web example smoke passed");
} finally {
  await browser?.close();
  server.kill();
}

async function importPlaywright() {
  try {
    return await import("playwright");
  } catch {
    return createRequire(import.meta.url)("playwright");
  }
}
