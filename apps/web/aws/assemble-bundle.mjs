// Assembles apps/web/.aws-lambda/ — the Lambda bundle the CDK WebStartStack deploys:
//   handler.mjs   (the Function-URL adapter, from aws/lambda-handler.mjs)
//   server/       (the WinterCG SSR build, from dist/server — self-contained via ssr.noExternal)
//   package.json  ("type":"module" so handler.mjs + server/server.js load as ESM)
// Run after `vite build` (the `build:aws` script does both).
//
// The web holds no database (P4: web-SSR-calls-Axum) — there's no seed to bake and no native binding
// to install, so the bundle is plain JS and the CDK ships it with no Docker bundling step.
import { cpSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";

const out = ".aws-lambda";
rmSync(out, { recursive: true, force: true });
mkdirSync(out, { recursive: true });
cpSync("aws/lambda-handler.mjs", `${out}/handler.mjs`);
cpSync("dist/server", `${out}/server`, { recursive: true });

// "type":"module" makes handler.mjs + server/server.js load as ESM. No dependencies: ssr.noExternal
// bundles everything into server.js, so there's no node_modules to install.
writeFileSync(`${out}/package.json`, `${JSON.stringify({ type: "module", private: true }, null, 2)}\n`);

// Inject the Apple Team ID into the AASA so macOS Password AutoFill can associate the desktop app with
// vegify.app (webcredentials). The committed source carries a __APPLE_TEAM_ID__ placeholder; CI sets
// VEGIFY_APPLE_TEAM_ID from the Apple signing secret. We write the result into dist/client (served from
// S3 via the /.well-known/* CloudFront behavior), independent of Vite's public-dir dotfile copy.
const teamId = process.env.VEGIFY_APPLE_TEAM_ID;
let aasa = readFileSync("public/.well-known/apple-app-site-association", "utf8");
if (teamId) {
  aasa = aasa.replaceAll("__APPLE_TEAM_ID__", teamId);
} else {
  console.warn("[assemble-bundle] VEGIFY_APPLE_TEAM_ID unset — AASA keeps its placeholder (autofill won't validate until it's set).");
}
mkdirSync("dist/client/.well-known", { recursive: true });
writeFileSync("dist/client/.well-known/apple-app-site-association", aasa);
console.warn(`[assemble-bundle] wrote dist/client/.well-known/apple-app-site-association (${teamId ? "Team ID injected" : "PLACEHOLDER"}).`);

console.warn(`[assemble-bundle] wrote ${out}/ (handler + server + package.json) — self-contained, no node_modules.`);
