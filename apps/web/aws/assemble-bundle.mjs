// Assembles apps/web/.aws-lambda/ — the Lambda bundle the CDK WebStartStack deploys:
//   handler.mjs   (the Function-URL adapter, from aws/lambda-handler.mjs)
//   server/       (the WinterCG SSR build, from dist/server — self-contained via ssr.noExternal)
//   package.json  ("type":"module" so handler.mjs + server/server.js load as ESM)
// Run after `vite build` (the `build:aws` script does both).
//
// The web holds no database (P4: web-SSR-calls-Axum) — there's no seed to bake and no native binding
// to install, so the bundle is plain JS and the CDK ships it with no Docker bundling step.
import { cpSync, mkdirSync, rmSync, writeFileSync } from "node:fs";

const out = ".aws-lambda";
rmSync(out, { recursive: true, force: true });
mkdirSync(out, { recursive: true });
cpSync("aws/lambda-handler.mjs", `${out}/handler.mjs`);
cpSync("dist/server", `${out}/server`, { recursive: true });

// "type":"module" makes handler.mjs + server/server.js load as ESM. No dependencies: ssr.noExternal
// bundles everything into server.js, so there's no node_modules to install.
writeFileSync(`${out}/package.json`, `${JSON.stringify({ type: "module", private: true }, null, 2)}\n`);

console.warn(`[assemble-bundle] wrote ${out}/ (handler + server + package.json) — self-contained, no node_modules.`);
