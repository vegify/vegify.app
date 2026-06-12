# Figma plugin: audit & restore Sketch text overrides

Development plugin that verifies (and where needed, repairs) the symbol-instance text content
from `Vegify.sketch` against the imported Figma file, using
[`design/figma-import/text-overrides.json`](../../design/figma-import/text-overrides.json)
(inlined into `code.js` at build time).

## Order of operations

1. **Missing-font replacement first.** The importer writes PostScript family tokens
   (`AvenirNext`, `ZapfDingbatsITC`, `SFProDisplay`, …) that don't match installed families, which
   makes text invisible. In Figma run the missing-fonts replacement **across all pages** mapping
   each token to its real family (`Avenir Next`, `Zapf Dingbats`, `SF Pro Display`, …).
2. **Audit (read-only).** Classifies every expected override:
   - `ok` — real content already present
   - `showsDefault` — node still shows the master default (Restore candidate)
   - `unknown` — shows neither (hand-edited or drifted; listed with current vs expected)
   - `notFound` / `containerMissing` / `pageMissing` — usually pruned pages or renamed beyond recognition
3. **Restore (writes)** — only touches nodes currently showing the recorded master default.
   Idempotent; reruns converge to `ok`.

## Run it

1. Open the Vegify file in **Figma desktop**, in design mode (not Dev Mode).
2. **Plugins → Development → Import plugin from manifest…** → this folder's `manifest.json`
   (only needed once).
3. Run the plugin; use the **Audit** button, read the panel/console, then **Restore** if needed.

## Matching model

Page name → artboard name → instance matched by **layer name OR component name** (the importer
renames custom instance names to the component's name, and detaches some scaled instances to
frames/groups), disambiguated by the instance's recorded canvas position from the .sketch, then
text node by name with a current-text-equals-default guard before any write.

## Rebuild after data changes

```sh
python3 tools/figma-restore-overrides/build.py
```

Figma re-reads `code.js` on each run; re-import is only needed if `manifest.json` changes.
