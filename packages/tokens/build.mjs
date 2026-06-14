// Generates dist/theme.css (Tailwind v4) from design/tokens/*.json (DTCG).
// Output has three concerns:
//   1. the brand ramp + type scale as Tailwind @theme tokens (the design source of truth);
//   2. the shadcn / Base UI semantic token contract (`--primary`, `--muted`, `--ring`, …)
//      mapped ONTO that brand ramp, so the ported base-nova components render on-brand;
//   3. the Base UI custom variants (`data-open:`, `data-checked:`, …) those components need.
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
  "",
  "/* 1. Brand ramp + type scale (the design source of truth). */",
  "@theme {",
];

for (const [name, t] of Object.entries(color)) {
  lines.push(`  --color-${name}: ${t.$value};`);
}
// semantic aliases on the brand ramp (primary is owned by the shadcn contract below)
lines.push("  --color-primary-dark: var(--color-green-dark);");
lines.push("  --color-primary-light: var(--color-green-light);");

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

// radius scale — overrides Tailwind's defaults so rounded-* match the brand, and
// emits --radius-* onto :root so components can reference var(--radius-md) directly.
lines.push("  --radius-sm: 0.375rem;");
lines.push("  --radius-md: 0.5rem;");
lines.push("  --radius-lg: 0.625rem;");
lines.push("  --radius-xl: 0.875rem;");
lines.push("}");
lines.push("");

// 2. shadcn / Base UI semantic contract, mapped onto the brand ramp.
//    Bare names live on :root so component classes can use var(--foreground) etc.
//    directly; @theme inline binds the Tailwind color-* utilities to them.
//    Note: --accent here is shadcn's neutral hover surface (NOT the brand orange,
//    which stays available as --color-orange).
lines.push("/* 2. shadcn / Base UI semantic tokens, mapped onto the brand ramp. */");
lines.push(":root {");
lines.push("  --background: #ffffff;");
lines.push("  --foreground: var(--color-gray-900);");
lines.push("  --card: #ffffff;");
lines.push("  --card-foreground: var(--color-gray-900);");
lines.push("  --popover: #ffffff;");
lines.push("  --popover-foreground: var(--color-gray-900);");
lines.push("  --primary: var(--color-green);");
lines.push("  --primary-foreground: #ffffff;");
lines.push("  --secondary: var(--color-gray-100);");
lines.push("  --secondary-foreground: var(--color-gray-900);");
lines.push("  --muted: var(--color-gray-100);");
lines.push("  --muted-foreground: var(--color-gray-500);");
lines.push("  --accent: var(--color-gray-100);");
lines.push("  --accent-foreground: var(--color-gray-900);");
lines.push("  --destructive: var(--color-red);");
lines.push("  --destructive-foreground: #ffffff;");
lines.push("  --border: var(--color-gray-100);");
lines.push("  --input: color-mix(in oklab, var(--color-gray-500) 22%, #ffffff);");
lines.push("  --ring: var(--color-green);");
lines.push("  --radius: 0.625rem;");
lines.push("}");
lines.push("");
lines.push("@theme inline {");
for (const t of [
  "background",
  "foreground",
  "card",
  "card-foreground",
  "popover",
  "popover-foreground",
  "primary",
  "primary-foreground",
  "secondary",
  "secondary-foreground",
  "muted",
  "muted-foreground",
  "accent",
  "accent-foreground",
  "destructive",
  "destructive-foreground",
  "border",
  "input",
  "ring",
]) {
  lines.push(`  --color-${t}: var(--${t});`);
}
lines.push("}");
lines.push("");

// 3. Base UI data-attribute variants the base-nova components rely on. Without
//    these, classes like `data-checked:bg-primary` / `data-open:animate-in`
//    silently do nothing.
const variants = {
  "data-open": ['&:where([data-state="open"])', '&:where([data-open]:not([data-open="false"]))'],
  "data-closed": ['&:where([data-state="closed"])', '&:where([data-closed]:not([data-closed="false"]))'],
  "data-checked": ['&:where([data-state="checked"])', '&:where([data-checked]:not([data-checked="false"]))'],
  "data-unchecked": ['&:where([data-state="unchecked"])', '&:where([data-unchecked]:not([data-unchecked="false"]))'],
  "data-selected": ['&:where([data-selected="true"])'],
  "data-disabled": ['&:where([data-disabled="true"])', '&:where([data-disabled]:not([data-disabled="false"]))'],
  "data-active": ['&:where([data-state="active"])', '&:where([data-active]:not([data-active="false"]))'],
  "data-horizontal": ['&:where([data-orientation="horizontal"])'],
  "data-vertical": ['&:where([data-orientation="vertical"])'],
  // base-nova extras (Base UI sets these bare data attributes):
  "data-inset": ['&:where([data-inset]:not([data-inset="false"]))'],
  "data-placeholder": ['&:where([data-placeholder])'],
  "data-popup-open": ['&:where([data-popup-open])'],
  "aria-invalid": ['&:where([aria-invalid="true"])'],
};
lines.push("/* 3. Base UI custom variants (from shadcn/tailwind.css + base-nova). */");
for (const [name, selectors] of Object.entries(variants)) {
  lines.push(`@custom-variant ${name} {`);
  lines.push(`  ${selectors.join(",\n  ")} {`);
  lines.push("    @slot;");
  lines.push("  }");
  lines.push("}");
}
lines.push("");

// Default border color to the token so bare `border` / `border-t` (Tailwind v4
// defaults to currentColor) match the brand instead of the text color.
lines.push("@layer base {");
lines.push("  *,");
lines.push("  ::before,");
lines.push("  ::after,");
lines.push("  ::backdrop {");
lines.push("    border-color: var(--border);");
lines.push("  }");
lines.push("}");

mkdirSync(join(here, "dist"), { recursive: true });
writeFileSync(join(here, "dist", "theme.css"), lines.join("\n") + "\n");
console.log(`theme.css written (${Object.keys(color).length} colors)`);
