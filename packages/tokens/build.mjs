// Generates dist/theme.css (Tailwind v4 @theme) from design/tokens/*.json (DTCG).
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const tokensDir = join(here, "..", "..", "design", "tokens");

const color = JSON.parse(readFileSync(join(tokensDir, "color.json"), "utf8")).color;
const typography = JSON.parse(
  readFileSync(join(tokensDir, "typography.json"), "utf8")
).typography;

const lines = [
  "/* Generated from design/tokens — do not edit by hand. `pnpm --filter @vegify/tokens build` */",
  "@theme {",
];

for (const [name, t] of Object.entries(color)) {
  lines.push(`  --color-${name}: ${t.$value};`);
}
// semantic aliases on the brand ramp
lines.push("  --color-primary: var(--color-green);");
lines.push("  --color-primary-dark: var(--color-green-dark);");
lines.push("  --color-primary-light: var(--color-green-light);");
lines.push("  --color-accent: var(--color-orange);");

// font stacks (Avenir Next is local/macOS; fallbacks for the web)
lines.push(
  '  --font-sans: "Avenir Next", Avenir, "Nunito Sans", system-ui, sans-serif;'
);
lines.push('  --font-serif: Adelle, Georgia, serif;');

// heading scale from the Sketch text styles (reference vars, not Tailwind text-* sizes)
const wanted = {
  "desktop-hd-heading-heading-1": "h1",
  "desktop-hd-heading-heading-2": "h2",
  "desktop-hd-heading-heading-3": "h3",
  "desktop-hd-heading-heading-4": "h4",
  "desktop-hd-body": "body",
};
for (const [key, alias] of Object.entries(wanted)) {
  const t = typography[key];
  if (t) lines.push(`  --font-size-${alias}: ${t.$value.fontSize};`);
}

lines.push("}");

mkdirSync(join(here, "dist"), { recursive: true });
writeFileSync(join(here, "dist", "theme.css"), lines.join("\n") + "\n");
console.log(`theme.css written (${Object.keys(color).length} colors)`);
