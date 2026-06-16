// Assembles apps/web-start/.aws-lambda/ — the Lambda bundle the CDK WebStartStack deploys:
//   handler.mjs  (the Function-URL adapter, from aws/lambda-handler.mjs)
//   server/      (the WinterCG SSR build, from dist/server)
// Run after `vite build` (the `build:aws` script does both).
//
// NOTE: this does NOT add node_modules. The server imports @libsql/client (a NATIVE binding)
// which must match the Lambda arch (arm64). Add it at deploy via Lambda-arch Docker bundling or
// the linux-arm64 prebuilt — see infra/README.md. macOS-installed binaries won't run on Lambda.
import { cpSync, mkdirSync, rmSync } from "node:fs";

const out = ".aws-lambda";
rmSync(out, { recursive: true, force: true });
mkdirSync(out, { recursive: true });
cpSync("aws/lambda-handler.mjs", `${out}/handler.mjs`);
cpSync("dist/server", `${out}/server`, { recursive: true });
console.warn(
  `[assemble-bundle] wrote ${out}/ (handler + server). Add node_modules with @libsql/client ` +
    `built for the Lambda arch before deploy — see infra/README.md.`,
);
