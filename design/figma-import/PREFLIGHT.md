# Vegify.sketch → Figma import preflight

Source: `~/Vegify Dropbox/vegifyapp/brand/Vegify.sketch` (192 MB, saved with Sketch 99.5, Apr 2024).
Raw inventory: [inventory.txt](inventory.txt). Extracted palette: [palette.json](palette.json).

## File facts

- 35 pages, 132 artboards + 245 symbol masters, 983 symbol instances (433 with overrides, 220 scaled)
- **Zero library references** (no foreign symbols/styles/swatches) — fully self-contained
- **Zero Smart Layout** usage — nothing to lose in the Smart Layout → Auto Layout gap
- Color space sRGB (matches Figma default — no color shift)
- 24 shared text styles + 21 layer styles (these convert to Figma styles), 12 color swatches (these do NOT — see below)
- 29 embedded bitmaps totaling ~193 MB uncompressed; effects are mundane (8 gaussian blurs, linear/radial gradients only)
- 145 prototype links (will not import), 199 layers with export settings, 813 boolean ops, 1 slice, 2 hotspots

## Fonts — RESOLVED

Embedded fonts were extracted from the .sketch and installed to `~/Library/Fonts`
(Adelle-Regular, Lato-Regular, Lato-Bold, SFProDisplay-Bold/Semibold/Heavy).
Avenir Next (the workhorse — ~1,000 text runs), ZapfDingbats, and Helvetica ship with macOS.

Only gap: `SFCompactDisplay-Regular`, used in **2** text runs. Ignore, or download Apple's SF fonts from developer.apple.com/fonts if it ever matters.

Re-extract anytime: the font binaries live inside the .sketch zip under `fonts/`, referenced from `document.json → fontReferences`. (Font binaries are deliberately not committed here — Adelle is commercially licensed.)

## Import runbook

1. Use the **Figma desktop app** (installed) so local fonts are picked up without the browser font-helper.
2. If on a free Starter plan, check page limits first — team files are capped at 3 pages and this file has 35. Import into Drafts or a Professional team.
3. Drag `Vegify.sketch` onto the Figma file browser. One import = one new Figma file. Expect several minutes at this size; let it finish.
4. **Import exactly once.** Re-importing creates a parallel file with new component IDs — nothing merges. The imported file becomes the working file; the .sketch in Dropbox stays the archive.
5. Immediately prune dead pages to keep the file fast (~193 MB of bitmaps): `Scraps`, `Old Video Graphics`, the three `Video Graphics - 20xx` pages, `xxx Page/...`, `Patreon Tiers` — whatever is truly dead. This is the bulk of the file weight.
6. Recreate the 12 swatches as Figma color variables from [palette.json](palette.json) (~5 min by hand; the Sketch names were just numbered hexes, so assign semantic names — the three greens are the brand ramp). Treat palette.json as the seed for code tokens (Tailwind config) too — repo stays the source of truth.
7. QA pass (see below), fixing text drift at the *style* level so it propagates.

## Known import casualty: instance text overrides

Confirmed post-import: Figma dropped symbol-instance overrides, so affected instances silently
show the master's default text. The full recovery reference is [text-overrides.md](text-overrides.md)
(392 real-content text overrides across 20 pages, extracted from the .sketch; complete JSON dump
alongside, including the 146 symbol-swap and 113 style overrides). Personas were rebuilt as docs
in [`../personas/`](../personas/).

## QA spot-check order

1. `Style Guide` page — type + color at a glance
2. `Brand Symbols` (10 masters) at high zoom — boolean-op logo marks are where path rendering can differ
3. `⚛️ Symbols` (187 masters) — scan thumbnails for anything visibly broken
4. One heavy flow, e.g. `Page/3.Recipes/2.0.Create or Edit Recipe` — text rewrap + the scaled instances

## Known losses (accepted)

- 145 prototype links — Figma does not import Sketch prototyping. Flows are 2024-era; rebuild in Figma only if needed.
- Export settings may not fully carry — re-add on assets at export time.
- Slices/hotspots (3 total) — negligible.

## Expected drift (not preventable, only correctable)

- ~940 text runs use Sketch *auto* line-height. Sketch and Figma compute auto line-height differently, so expect small vertical shifts and occasional rewraps, mostly in Avenir Next. Fix once per text style (24 exist) rather than per layer.
- 220 scaled symbol instances may land at slightly odd sizes — covered by QA step 4.
