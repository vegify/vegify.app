// AWS Lambda adapter for TanStack Start's WinterCG fetch handler — the FREE-TIER hosting path.
//
// The CDK WebStartStack packages this bundle as { handler.mjs, server/, vegify-seed.db,
// node_modules/@libsql/client } and mounts EFS at /mnt/data. The DB lives on EFS (the only
// persistent store a stateless, scale-to-zero Lambda can WRITE to for free) at
// DATABASE_URL=file:/mnt/data/vegify.db.
//
// Two EFS-specific details, both load-bearing:
//   1. Seed-on-first-boot. EFS starts empty, so on the first cold start we copy the baked
//      vegify-seed.db onto EFS, then import the server (which opens the DB). The import is
//      DYNAMIC and below the copy so @vegify/db connects only after the file exists.
//   2. Rollback journal, not WAL. The seed is baked in DELETE (rollback) journal mode because
//      WAL needs an mmap'd shared-memory file that NFS/EFS can't provide. The Lambda is pinned to
//      reserved concurrency 1 (CDK), so a single writer over EFS is safe.
import { existsSync, copyFileSync, mkdirSync } from "node:fs";
import { dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { ensureAuthSchema } from "./ensure-schema.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const dbPath = (process.env.DATABASE_URL ?? "file:/mnt/data/vegify.db").replace(/^file:/, "");
if (!existsSync(dbPath)) {
  mkdirSync(dirname(dbPath), { recursive: true });
  copyFileSync(`${here}/vegify-seed.db`, dbPath); // seed EFS from the baked rollback-mode copy
}

// EFS persists across deploys and the seed-copy above only runs on an EMPTY volume, so a DB seeded
// before the auth schema existed must be migrated in place (additive, idempotent) before the app opens it.
await ensureAuthSchema(dbPath);

// Dynamic import AFTER the seed copy + migration — @vegify/db connects at module load.
const app = await import("./server/server.js");
const target = app.default ?? app;
const fetchHandler = typeof target === "function" ? target : target.fetch.bind(target);

export const handler = async (event) => {
  const { rawPath = "/", rawQueryString = "", headers = {}, cookies, body, isBase64Encoded } = event;
  const method = event.requestContext?.http?.method ?? "GET";
  const host = headers["x-forwarded-host"] || headers.host || event.requestContext?.domainName || "localhost";
  const proto = headers["x-forwarded-proto"] || "https";
  const url = `${proto}://${host}${rawPath}${rawQueryString ? `?${rawQueryString}` : ""}`;

  const h = new Headers();
  for (const [k, v] of Object.entries(headers)) if (v != null) h.set(k, v);
  if (cookies?.length) h.set("cookie", cookies.join("; "));

  let reqBody;
  if (method !== "GET" && method !== "HEAD" && body != null) {
    reqBody = isBase64Encoded ? Buffer.from(body, "base64") : body;
  }

  const response = await fetchHandler(
    new Request(url, { method, headers: h, body: reqBody, duplex: "half" }),
  );

  const respHeaders = {};
  const setCookie = [];
  response.headers.forEach((val, key) => {
    if (key.toLowerCase() === "set-cookie") setCookie.push(val);
    else respHeaders[key] = val;
  });
  const buf = Buffer.from(await response.arrayBuffer());
  return {
    statusCode: response.status,
    headers: respHeaders,
    cookies: setCookie.length ? setCookie : undefined,
    body: buf.toString("base64"),
    isBase64Encoded: true,
  };
};
