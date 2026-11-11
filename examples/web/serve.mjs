// Static file server for the example. Serves the repository root so the page
// can import runtimes/web and fetch a committed .kvk. Local use only:
// binds to 127.0.0.1.
//
//   node examples/web/serve.mjs [port]     then open the printed URL
import { createReadStream, statSync } from "node:fs";
import { createServer } from "node:http";
import { extname, join, normalize, resolve, sep } from "node:path";

const root = resolve(new URL("../..", import.meta.url).pathname);
const port = Number(process.argv[2] ?? process.env.PORT ?? 8787);
const types = {
  ".html": "text/html; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".mjs": "text/javascript; charset=utf-8",
  ".wasm": "application/wasm",
  ".kvk": "application/octet-stream",
  ".json": "application/json",
};

const server = createServer((request, response) => {
  const path = decodeURIComponent(new URL(request.url, "http://localhost").pathname);
  const file = normalize(join(root, path.endsWith("/") ? `${path}index.html` : path));
  if (file !== root && !file.startsWith(root + sep)) {
    response.writeHead(403).end();
    return;
  }
  try {
    if (!statSync(file).isFile()) throw new Error("not a file");
  } catch {
    response.writeHead(404).end("not found");
    return;
  }
  response.writeHead(200, { "content-type": types[extname(file)] ?? "application/octet-stream" });
  createReadStream(file).pipe(response);
});

server.listen(port, "127.0.0.1", () => {
  console.log(`http://127.0.0.1:${port}/examples/web/`);
});
