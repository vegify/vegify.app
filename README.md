# vegify.app

## Design

The living design lives in Figma, binary archives live in Dropbox, and this repo holds the diffable design-to-code contract.

- **Figma file:** [Vegify](https://www.figma.com/design/FIGMA_FILE_KEY/Vegify) (file key `FIGMA_FILE_KEY`)
- **Binary archives:** `~/Vegify Dropbox/vegifyapp/brand/` — `.sketch`/`.fig`/`.psd` are gitignored by policy
- **Design tokens:** [`design/tokens/`](design/tokens/) — `color.json` and `typography.json` in W3C DTCG format, extracted from the Sketch shared styles; seed for the eventual Tailwind config
- **Figma import runbook:** [`design/figma-import/PREFLIGHT.md`](design/figma-import/PREFLIGHT.md)
- **Text-override reference:** [`design/figma-import/text-overrides.md`](design/figma-import/text-overrides.md) — the real text content of every symbol instance; the Figma import dropped these overrides, so affected instances show master defaults until fixed
- **Personas:** [`design/personas/`](design/personas/)
- **Site map:** [`design/site-map.md`](design/site-map.md) — mermaid diagram reconstructed from the Sketch `Site Map` page, numbering preserved

## Development

pnpm + Turborepo. Two app shells implement the same recipes slice against shared packages —
a deliberate Next.js vs TanStack Start benchmark; everything except the framework is shared.

```
apps/web-next    Next.js (App Router)        pnpm --filter web-next dev    → :3000
apps/web-start   TanStack Start (Vite)       pnpm --filter web-start dev   → :3001
packages/db      Drizzle + libSQL/SQLite — schema ported from vegify-laravel
packages/ui      shared components (placeholder primitives until the shadcn/Base UI pass)
packages/tokens  design/tokens → Tailwind v4 @theme (build: pnpm --filter @vegify/tokens build)
```

First run:

```sh
pnpm install
pnpm --filter @vegify/tokens build   # design tokens → theme.css
pnpm db:push                         # create .data/vegify.db from the Drizzle schema
pnpm db:seed                         # Biga + Neapolitan pizza dough sample data
pnpm dev                             # or build: pnpm build
```

Database: local dev uses a SQLite file at `.data/vegify.db` (no daemon). Prod target is
Turso/libSQL or Cloudflare D1 — set `DATABASE_URL` (+ `DATABASE_AUTH_TOKEN`); same client.
The schema stays portable to Postgres if the site ever outgrows hobby infra.
