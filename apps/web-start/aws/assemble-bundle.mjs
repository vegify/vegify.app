// Assembles apps/web-start/.aws-lambda/ — the Lambda bundle the CDK WebStartStack deploys:
//   handler.mjs      (the Function-URL adapter, from aws/lambda-handler.mjs)
//   server/          (the WinterCG SSR build, from dist/server)
//   vegify-seed.db   (the seed DB the handler copies onto EFS on first cold start)
//   package.json     ("type":"module" so server.js is ESM; declares the @libsql/client dep)
// Run after `vite build` (the `build:aws` script does both).
//
// The seed is baked in DELETE (rollback) journal mode — EFS/NFS can't back WAL's shared-memory
// file, so the on-EFS DB must not use WAL (see aws/lambda-handler.mjs).
//
// node_modules is NOT added here: @libsql/client is a NATIVE binding that must match the Lambda
// arch. The CDK installs it from this package.json at deploy via Docker bundling (x86_64) — see
// infra/README.md. macOS-installed binaries won't run on Lambda.
import { cpSync, mkdirSync, rmSync, writeFileSync } from "node:fs";
import { execFileSync } from "node:child_process";
import { resolve } from "node:path";

const out = ".aws-lambda";
rmSync(out, { recursive: true, force: true });
mkdirSync(out, { recursive: true });
cpSync("aws/lambda-handler.mjs", `${out}/handler.mjs`);
cpSync("dist/server", `${out}/server`, { recursive: true });

// "type":"module" makes handler.mjs + server/server.js ESM; @libsql/client stays external (native).
writeFileSync(
  `${out}/package.json`,
  `${JSON.stringify(
    {
      type: "module",
      private: true,
      // The only deps the SSR build leaves external (everything else is bundled via ssr.noExternal):
      // the native libSQL client and drizzle-orm (its /sqlite-core subpath stays external).
      dependencies: { "@libsql/client": "^0.17.3", "drizzle-orm": "^0.45.2" },
    },
    null,
    2,
  )}\n`,
);

// Bake the seed DB. `VACUUM INTO` writes a clean single-file copy (no WAL/SHM sidecars); then
// force rollback-journal mode on that copy so it's EFS-safe the moment it's opened.
const seedSrc = resolve("../../.data/vegify.db");
const seedOut = `${out}/vegify-seed.db`;
execFileSync("sqlite3", [seedSrc, `VACUUM INTO '${resolve(seedOut)}'`]);
execFileSync("sqlite3", [seedOut, "PRAGMA journal_mode=DELETE;"]);

console.warn(
  `[assemble-bundle] wrote ${out}/ (handler + server + vegify-seed.db + package.json). The CDK ` +
    `installs @libsql/client (x86_64) via Docker bundling at deploy — see infra/README.md.`,
);
