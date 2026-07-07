import { readdirSync } from "node:fs";
import tailwindcss from "@tailwindcss/vite";
import { devtools } from "@tanstack/devtools-vite";
import { tanstackStart } from "@tanstack/react-start/plugin/vite";
import viteReact from "@vitejs/plugin-react";
import { defineConfig } from "vite";

// Route filenames, read once at config time and baked into the bundle as __VEGIFY_ROUTE_FILES__. The auth
// gate (src/auth-gate.ts) derives which top-level paths are STATIC routes from this list — everything else
// single-segment is a "/<username>" profile — so adding a route file auto-gates it. We read the directory
// here rather than import.meta.glob in the gate: globbing the route modules (which the route tree already
// imports) emits noisy INEFFECTIVE_DYNAMIC_IMPORT warnings, and importing the route tree would be circular.
const routeFiles = readdirSync(new URL("./src/routes", import.meta.url));

const config = defineConfig({
  // Uncommon pinned dev port; strict so a taken port fails loudly instead of
  // silently shifting. (The Fargate/container PORT in infra is unrelated.)
  server: { port: 47307, strictPort: true },
  resolve: { tsconfigPaths: true },
  define: { __VEGIFY_ROUTE_FILES__: JSON.stringify(routeFiles) },
  // Bundle every dep INTO the SSR build (incl. react + the @vegify/* workspace packages) so the
  // deployed server.js is self-contained. The web holds no database — it calls the Axum backend over
  // HTTP (VEGIFY_API_URL) — so there's no native @libsql binding left to keep external.
  ssr: { noExternal: true },
  // React Compiler runs as a Babel plugin inside @vitejs/plugin-react (target React 19 → no runtime dep).
  plugins: [
    devtools(),
    tailwindcss(),
    tanstackStart(),
    // @vitejs/plugin-react v6's Options type omits `babel`, but it's the canonical React Compiler setup
    // and the build applies it. Drop this directive once the plugin's types expose `babel` again.
    viteReact({
      // @ts-expect-error -- babel option missing from plugin-react v6 Options type
      babel: { plugins: [["babel-plugin-react-compiler", { target: "19" }]] },
    }),
  ],
});

export default config;
