# vegify.app

## Design

The living design lives in Figma, binary archives live in Dropbox, and this repo holds the diffable design-to-code contract.

- **Figma file:** [Vegify](https://www.figma.com/design/FIGMA_FILE_KEY/Vegify) (file key `FIGMA_FILE_KEY`)
- **Binary archives:** `~/Vegify Dropbox/vegifyapp/brand/` — `.sketch`/`.fig`/`.psd` are gitignored by policy
- **Design tokens:** [`design/tokens/`](design/tokens/) — `color.json` and `typography.json` in W3C DTCG format, extracted from the Sketch shared styles; seed for the eventual Tailwind config
- **Figma import runbook:** [`design/figma-import/PREFLIGHT.md`](design/figma-import/PREFLIGHT.md)
- **Text-override reference:** [`design/figma-import/text-overrides.md`](design/figma-import/text-overrides.md) — the real text content of every symbol instance; the Figma import dropped these overrides, so affected instances show master defaults until fixed
- **Personas:** [`design/personas/`](design/personas/)
