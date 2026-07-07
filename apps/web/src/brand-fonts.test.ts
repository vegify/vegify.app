// Locks the brand-serif pipeline: the serif headings must render a face we actually SHIP, on every
// platform. History: --font-serif led with Adelle, which only ever existed hand-installed on dev
// Macs (Regular weight, faux-bolded) — iOS and every public-web visitor silently fell back to
// Georgia. The fix ships Bitter (OFL) via @fontsource, bundled by Vite from theme.css. This test
// is the CI-able seam (content + resolvability); actual glyph rendering can only be verified in a
// running WebKit, which is covered by the simulator/desktop smoke.

import { readFileSync } from "node:fs";
import { createRequire } from "node:module";
import { describe, expect, it } from "vitest";

const require = createRequire(import.meta.url);
const themeCssPath = require.resolve("@vegify/tokens/theme.css");
const themeCss = readFileSync(themeCssPath, "utf8");

describe("brand serif ships with the bundle", () => {
  it("leads the serif stack with Bitter (the shipped face), keeping Adelle as a local preference", () => {
    const stack = themeCss.match(/--font-serif:\s*([^;]+);/)?.[1];
    if (!stack) throw new Error("--font-serif not found in theme.css");
    expect(stack.trim().startsWith("Bitter")).toBe(true);
    expect(stack).toContain("Georgia"); // last-resort fallbacks stay
  });

  it("imports the @fontsource faces for every weight the screens use (400/600/700)", () => {
    for (const w of [400, 600, 700]) {
      expect(themeCss).toContain(`@import "@fontsource/bitter/${w}.css";`);
    }
  });

  it("resolves @fontsource/bitter from the tokens package (pnpm gives no hoisting freebies)", () => {
    const fromTokens = createRequire(themeCssPath);
    for (const w of [400, 600, 700]) {
      expect(() =>
        fromTokens.resolve(`@fontsource/bitter/${w}.css`),
      ).not.toThrow();
    }
  });
});
