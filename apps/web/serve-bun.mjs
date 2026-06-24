// Production runtime for web: serves the built WinterCG fetch handler on Bun.
//   pnpm --filter web build           # produces dist/server + dist/client
//   pnpm --filter web start:bun        # serve the build on Bun (PORT, DATABASE_URL via env)
//
// Why Bun: measured ~+10% read throughput and a ~10x tighter p99.9 tail vs Node serving the SAME
// build under sustained concurrency — the win is the runtime (JSC vs V8), not native serving (a
// node:http bridge on Bun captures nearly all of it). See docs/benchmark.md "Runtime: Node vs Bun".
// Native @libsql/client loads fine on Bun. This file is also the basis for the AWS Fargate
// entrypoint (a long-running Bun server in the DB's VPC — the model where the win is realized).
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const serverPath = process.env.SERVER_PATH ?? join(here, "dist/server/server.js");
const clientDir = join(dirname(dirname(serverPath)), "client");
const port = Number(process.env.PORT ?? 3001);

const mod = await import(serverPath);
const app = mod.default;
const fetchHandler = typeof app === "function" ? app : app.fetch.bind(app);

// Serve built static client assets directly (a CDN/CloudFront does this in real prod);
// everything else goes to the app's fetch handler.
Bun.serve({
  port,
  idleTimeout: 30,
  async fetch(req) {
    const pathname = new URL(req.url).pathname;
    if (pathname !== "/" && !pathname.endsWith("/")) {
      const file = Bun.file(join(clientDir, pathname));
      if (await file.exists()) return new Response(file);
    }
    return fetchHandler(req);
  },
});
console.log(`web (Bun.serve) listening on :${port}`);
