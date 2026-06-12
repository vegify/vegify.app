# Figma plugin: restore Sketch text overrides

One-shot development plugin that repairs the text content Figma's `.sketch` import dropped
(symbol-instance overrides). It replays all 392 real overrides from
[`design/figma-import/text-overrides.json`](../../design/figma-import/text-overrides.json),
inlined into `code.js` at build time.

## Run it

1. Open the Vegify file in the **Figma desktop app**.
2. Menu → **Plugins → Development → Import plugin from manifest…** → select this folder's `manifest.json`.
3. Run **Vegify — Restore Sketch Text Overrides** (Plugins → Development).
4. Toast shows totals; **Plugins → Development → Show/Hide console** has the per-node detail,
   including anything it deliberately left alone.

## Safety model

- A text node is written **only if** it currently shows the master default recorded at extraction
  time (exact match, falling back to a trimmed match) — i.e. only import-damaged nodes.
- Nodes already showing the target text are counted as `alreadyCorrect`, not rewritten.
- Anything ambiguous (multiple matching nodes), hand-edited (matches neither default nor target),
  or missing (e.g. on pages you pruned) is reported in the console and left untouched.
- Idempotent — run it repeatedly; reruns converge to `alreadyCorrect`.

Matching is by page name → artboard/frame name → instance name (disambiguated by component name)
→ text-layer name, with the current-text-equals-default check as the final guard.

## Rebuild after data changes

```sh
python3 tools/figma-restore-overrides/build.py
```

Regenerates `code.js` from `plugin.src.js` + the JSON. Figma picks up the change on next run
(no re-import needed).
